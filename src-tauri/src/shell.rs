//! Opt-in "Open in Alterix" file-manager integration, installed and removed from
//! Settings. This is the only feature that writes outside the data folder, and only
//! when the user turns it on:
//! - Windows: context-menu keys under HKEY_CURRENT_USER\Software\Classes.
//! - macOS: a Finder Quick Action in ~/Library/Services.
//! - Linux: a Nautilus script, a Dolphin service menu and an "Open With" entry.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellStatus {
    pub supported: bool,
    pub installed: bool,
    pub places: Vec<String>,
}

/// The program the file manager should start.
fn launcher() -> Result<PathBuf, String> {
    if let Ok(appimage) = std::env::var("APPIMAGE") {
        return Ok(PathBuf::from(appimage));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe.canonicalize().unwrap_or(exe))
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const KEYS: [(&str, &str); 3] = [
        (r"HKCU\Software\Classes\*\shell\Alterix", "%1"),
        (r"HKCU\Software\Classes\Directory\shell\Alterix", "%1"),
        (
            r"HKCU\Software\Classes\Directory\Background\shell\Alterix",
            "%V",
        ),
    ];

    fn reg(args: &[&str]) -> Result<(), String> {
        let out = Command::new("reg")
            .args(args)
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    pub fn status() -> ShellStatus {
        let installed = reg(&["query", KEYS[0].0]).is_ok();
        ShellStatus {
            supported: true,
            installed,
            places: KEYS.iter().map(|k| k.0.to_string()).collect(),
        }
    }

    pub fn install() -> Result<ShellStatus, String> {
        let exe = launcher()?.to_string_lossy().into_owned();
        for (key, arg) in KEYS {
            reg(&["add", key, "/ve", "/d", "Open in Alterix", "/f"])?;
            reg(&["add", key, "/v", "Icon", "/d", &format!("\"{exe}\""), "/f"])?;
            reg(&[
                "add",
                &format!(r"{key}\command"),
                "/ve",
                "/d",
                &format!("\"{exe}\" \"{arg}\""),
                "/f",
            ])?;
        }
        Ok(status())
    }

    pub fn uninstall() -> Result<ShellStatus, String> {
        for (key, _) in KEYS {
            let _ = reg(&["delete", key, "/f"]);
        }
        Ok(status())
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    fn workflow_dir() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Services/Open in Alterix.workflow"))
    }

    fn app_bundle() -> Result<PathBuf, String> {
        let exe = launcher()?;
        exe.ancestors()
            .find(|p| p.extension().map(|e| e == "app").unwrap_or(false))
            .map(PathBuf::from)
            .ok_or_else(|| {
                "Run Alterix from its .app bundle to add the Finder Quick Action.".to_string()
            })
    }

    const INFO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>NSServices</key>
	<array>
		<dict>
			<key>NSMenuItem</key>
			<dict><key>default</key><string>Open in Alterix</string></dict>
			<key>NSMessage</key>
			<string>runWorkflowAsService</string>
			<key>NSRequiredContext</key>
			<dict><key>NSApplicationIdentifier</key><string>com.apple.finder</string></dict>
			<key>NSSendFileTypes</key>
			<array><string>public.item</string></array>
		</dict>
	</array>
</dict>
</plist>
"#;

    fn wflow(app: &str) -> String {
        let cmd = format!("open -n -a \"{}\" --args \"$@\"", app.replace('"', "\\\""));
        let cmd = cmd
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AMApplicationBuild</key><string>523</string>
	<key>AMApplicationVersion</key><string>2.10</string>
	<key>AMDocumentVersion</key><string>2</string>
	<key>actions</key>
	<array>
		<dict>
			<key>action</key>
			<dict>
				<key>AMAccepts</key>
				<dict>
					<key>Container</key><string>List</string>
					<key>Optional</key><true/>
					<key>Types</key><array><string>com.apple.cocoa.string</string></array>
				</dict>
				<key>AMActionVersion</key><string>2.0.3</string>
				<key>AMApplication</key><array><string>Automator</string></array>
				<key>AMParameterProperties</key>
				<dict>
					<key>COMMAND_STRING</key><dict/>
					<key>CheckedForUserDefaultShell</key><dict/>
					<key>inputMethod</key><dict/>
					<key>shell</key><dict/>
					<key>source</key><dict/>
				</dict>
				<key>AMProvides</key>
				<dict>
					<key>Container</key><string>List</string>
					<key>Types</key><array><string>com.apple.cocoa.string</string></array>
				</dict>
				<key>ActionBundlePath</key><string>/System/Library/Automator/Run Shell Script.action</string>
				<key>ActionName</key><string>Run Shell Script</string>
				<key>ActionParameters</key>
				<dict>
					<key>COMMAND_STRING</key><string>{cmd}</string>
					<key>CheckedForUserDefaultShell</key><true/>
					<key>inputMethod</key><integer>1</integer>
					<key>shell</key><string>/bin/sh</string>
					<key>source</key><string></string>
				</dict>
				<key>BundleIdentifier</key><string>com.apple.RunShellScript</string>
				<key>CFBundleVersion</key><string>2.0.3</string>
				<key>CanShowSelectedItemsWhenRun</key><false/>
				<key>CanShowWhenRun</key><true/>
				<key>Category</key><array><string>AMCategoryUtilities</string></array>
				<key>Class Name</key><string>RunShellScriptAction</string>
				<key>InputUUID</key><string>A1B2C3D4-0000-4000-8000-000000000001</string>
				<key>Keywords</key><array><string>Shell</string><string>Script</string></array>
				<key>OutputUUID</key><string>A1B2C3D4-0000-4000-8000-000000000002</string>
				<key>UUID</key><string>A1B2C3D4-0000-4000-8000-000000000003</string>
				<key>UnlocalizedApplications</key><array><string>Automator</string></array>
				<key>arguments</key><dict/>
				<key>isViewVisible</key><integer>1</integer>
				<key>location</key><string>309.000000:253.000000</string>
				<key>nibPath</key><string>/System/Library/Automator/Run Shell Script.action/Contents/Resources/Base.lproj/main.nib</string>
			</dict>
			<key>isViewVisible</key><integer>1</integer>
		</dict>
	</array>
	<key>connectors</key><dict/>
	<key>workflowMetaData</key>
	<dict>
		<key>applicationBundleIDsByPath</key><dict/>
		<key>applicationPaths</key><array/>
		<key>inputTypeIdentifier</key><string>com.apple.Automator.fileSystemObject</string>
		<key>outputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
		<key>presentationMode</key><integer>15</integer>
		<key>processesInput</key><false/>
		<key>serviceApplicationBundleID</key><string>com.apple.finder</string>
		<key>serviceApplicationPath</key><string>/System/Library/CoreServices/Finder.app</string>
		<key>serviceInputTypeIdentifier</key><string>com.apple.Automator.fileSystemObject</string>
		<key>serviceOutputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
		<key>serviceProcessesInput</key><false/>
		<key>systemImageName</key><string>NSActionTemplate</string>
		<key>useAutomaticInputType</key><false/>
		<key>workflowTypeIdentifier</key><string>com.apple.Automator.servicesMenu</string>
	</dict>
</dict>
</plist>
"#
        )
    }

    pub fn status() -> ShellStatus {
        let dir = workflow_dir();
        ShellStatus {
            supported: true,
            installed: dir.as_ref().map(|d| d.exists()).unwrap_or(false),
            places: dir
                .map(|d| d.to_string_lossy().into_owned())
                .into_iter()
                .collect(),
        }
    }

    pub fn install() -> Result<ShellStatus, String> {
        let app = app_bundle()?;
        let dir = workflow_dir().ok_or("No home folder.")?;
        let contents = dir.join("Contents");
        std::fs::create_dir_all(&contents).map_err(|e| e.to_string())?;
        std::fs::write(contents.join("Info.plist"), INFO).map_err(|e| e.to_string())?;
        std::fs::write(
            contents.join("document.wflow"),
            wflow(&app.to_string_lossy()),
        )
        .map_err(|e| e.to_string())?;
        // Ask macOS to pick up the new service.
        let _ = std::process::Command::new("/System/Library/CoreServices/pbs")
            .arg("-update")
            .output();
        Ok(status())
    }

    pub fn uninstall() -> Result<ShellStatus, String> {
        if let Some(d) = workflow_dir() {
            if d.exists() {
                std::fs::remove_dir_all(&d).map_err(|e| e.to_string())?;
            }
        }
        let _ = std::process::Command::new("/System/Library/CoreServices/pbs")
            .arg("-update")
            .output();
        Ok(status())
    }

    #[cfg(test)]
    pub fn workflow_text(app: &str) -> String {
        wflow(app)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn data_home() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }

    fn places() -> Vec<PathBuf> {
        data_home()
            .map(|d| {
                vec![
                    d.join("nautilus/scripts/Open in Alterix"),
                    d.join("kio/servicemenus/alterix.desktop"),
                    d.join("applications/alterix-open.desktop"),
                ]
            })
            .unwrap_or_default()
    }

    pub fn status() -> ShellStatus {
        let p = places();
        ShellStatus {
            supported: !p.is_empty(),
            installed: p.iter().any(|x| x.exists()),
            places: p.iter().map(|x| x.to_string_lossy().into_owned()).collect(),
        }
    }

    pub fn install() -> Result<ShellStatus, String> {
        let exe = launcher()?.to_string_lossy().replace('"', "\\\"");
        let p = places();
        if p.len() != 3 {
            return Err("No home folder.".into());
        }
        let write = |path: &PathBuf, text: String, mode: u32| -> Result<(), String> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(path, text).map_err(|e| e.to_string())?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
                .map_err(|e| e.to_string())
        };
        write(
            &p[0],
            format!("#!/bin/sh\n# Opens the selected items in Alterix.\nexec \"{exe}\" \"$@\"\n"),
            0o755,
        )?;
        write(
            &p[1],
            format!(
                "[Desktop Entry]\nType=Service\nMimeType=all/all;\nActions=openInAlterix\nX-KDE-ServiceTypes=KonqPopupMenu/Plugin\n\n[Desktop Action openInAlterix]\nName=Open in Alterix\nExec=\"{exe}\" %F\n"
            ),
            0o755,
        )?;
        write(
            &p[2],
            format!(
                "[Desktop Entry]\nType=Application\nName=Alterix\nComment=Bulk rename files and folders\nExec=\"{exe}\" %F\nMimeType=inode/directory;application/octet-stream;\nNoDisplay=true\nTerminal=false\n"
            ),
            0o644,
        )?;
        Ok(status())
    }

    pub fn uninstall() -> Result<ShellStatus, String> {
        for p in places() {
            let _ = std::fs::remove_file(p);
        }
        Ok(status())
    }
}

pub use imp::{install, status, uninstall};

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn workflow_is_a_valid_plist() {
        let dir = std::env::temp_dir().join(format!("alterix-wflow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("document.wflow");
        std::fs::write(
            &p,
            super::imp::workflow_text("/Applications/Alterix & Co.app"),
        )
        .unwrap();
        let out = std::process::Command::new("plutil")
            .arg("-lint")
            .arg(&p)
            .output()
            .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}
