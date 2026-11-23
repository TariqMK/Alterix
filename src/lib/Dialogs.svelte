<!-- All dialogs: settings, history (undo), rename pairs, properties, name explanation,
     rename report, confirm-each, jump to folder and rename by hand. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { readText } from "@tauri-apps/plugin-clipboard-manager";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { summary, type UpdateInfo } from "./updateCheck";
  import Modal from "./Modal.svelte";
  import { app, isMac } from "./state.svelte";
  import { COLUMNS, type ColumnId, type PreviewItem, type Row } from "./types";

  const close = () => {
    app.dialog = null;
    app.dialogData = null;
  };

  let tab = $state<"renaming" | "display" | "general" | "messages" | "data">(
    "renaming",
  );
  let shell = $state<{
    supported: boolean;
    installed: boolean;
    places: string[];
  } | null>(null);
  let props = $state<Record<string, string> | null>(null);
  let propImage = $state<string | null>(null);
  let propFilter = $state("");
  let jumpPath = $state("");
  let manualName = $state("");
  let confirmIndex = $state(0);
  let confirmSkip = $state<string[]>([]);

  $effect(() => {
    if (app.dialog === "settings" && tab === "data")
      invoke("shell_status").then((s) => (shell = s as typeof shell));
    if (app.dialog === "history") app.loadHistory();
    if (app.dialog === "properties" && typeof app.dialogData === "string") {
      const p = app.dialogData;
      props = null;
      propImage = null;
      invoke<Record<string, string>>("properties", { path: p }).then(
        (v) => (props = v),
      );
      invoke<string | null>("image_preview", { path: p }).then(
        (v) => (propImage = v),
      );
    }
    if (app.dialog === "manual" && app.dialogData)
      manualName = (app.dialogData as Row).name;
    if (app.dialog === "confirmEach") {
      confirmIndex = 0;
      confirmSkip = [];
    }
    if (app.dialog === "jump") jumpPath = app.folders[0] ?? "";
  });

  const settingsChanged = () => app.saveSettings();
  const fileManagerTip = navigator.userAgent.includes("Mac")
    ? "Show the log file in Finder"
    : navigator.userAgent.includes("Windows")
      ? "Show the log file in File Explorer"
      : "Show the log file in the file manager";
  const update = $derived(
    app.dialog === "update"
      ? (app.dialogData as
          (UpdateInfo & { error?: undefined }) | { error: string })
      : null,
  );

  function toggleColumn(c: ColumnId, on: boolean) {
    const cols = app.settings.columns.filter((x) => x !== c);
    if (on) {
      const order = COLUMNS.map((x) => x.id);
      cols.push(c);
      cols.sort((a, b) => order.indexOf(a) - order.indexOf(b));
    }
    app.settings.columns = cols;
    settingsChanged();
    app.loadExtraColumns();
  }

  async function pickDataFolder() {
    const dir = await open({
      directory: true,
      title: "Choose where Alterix keeps its data",
    });
    if (typeof dir !== "string") return;
    app.dataStatus = await invoke("use_data_folder", { path: dir });
    settingsChanged();
  }

  async function saveScript(id: string) {
    const win = !isMac() && navigator.userAgent.includes("Windows");
    const path = await save({
      title: "Save undo script",
      defaultPath: win ? "alterix-undo.bat" : "alterix-undo.sh",
    });
    if (path) {
      await invoke("save_undo_script", { id, path });
      app.say("Undo script saved.");
    }
  }

  async function importPairsFile() {
    const path = await open({
      title: "Import rename pairs",
      filters: [{ name: "Text or CSV", extensions: ["txt", "csv", "tsv"] }],
    });
    if (typeof path !== "string") return;
    await app.importPairs(await invoke("read_text_file", { path }));
  }

  const confirmItems = $derived((app.dialogData as PreviewItem[] | null) ?? []);
  function confirmAnswer(a: "yes" | "no" | "all" | "cancel") {
    const cur = confirmItems[confirmIndex];
    if (a === "cancel") return close();
    if (a === "no" && cur) confirmSkip.push(cur.path);
    if (a === "all" || confirmIndex >= confirmItems.length - 1) {
      const skip = [...confirmSkip];
      close();
      app.doRename(skip);
      return;
    }
    confirmIndex++;
  }

  const propRows = $derived(
    Object.entries(props ?? {}).filter(
      ([k, v]) =>
        !propFilter ||
        `${k} ${v}`.toLowerCase().includes(propFilter.toLowerCase()),
    ),
  );
</script>

{#if app.dialog === "settings"}
  <Modal title="Settings" onclose={close} wide>
    <div class="tabs" role="tablist">
      {#each [["renaming", "Renaming"], ["display", "Display"], ["general", "General"], ["messages", "Messages"], ["data", "Data and integration"]] as [id, label] (id)}
        <button
          role="tab"
          aria-selected={tab === id}
          class:active={tab === id}
          onclick={() => (tab = id as typeof tab)}
        >
          {label}
        </button>
      {/each}
    </div>

    {#if tab === "renaming"}
      <div class="form">
        <h3>Names</h3>
        <label class="check">
          <input type="checkbox" bind:checked={app.options.extensionsInName} /> Treat
          file extensions as part of the name
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.options.folderExtensionsInName}
          /> Treat dots in folder names as part of the name
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.options.allowFoldersInNames}
          /> Create folders from / or \ in new names
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={app.options.relaxValidation} /> Only
          check names against this computer's rules
        </label>
        <p class="muted small">
          By default Alterix rejects names that would break on Windows, macOS or
          Linux.
        </p>

        <h3>When a name is taken</h3>
        <label class="check">
          <input type="checkbox" bind:checked={app.options.preventDuplicates} /> Add
          a counter to make names unique
        </label>
        {#if app.options.preventDuplicates}
          <div class="indent">
            <label class="sel">
              <span>Counter format</span><input
                bind:value={app.options.duplicateFormat}
                class="mono"
              />
            </label>
            <p class="muted small">
              %name% is the name. %count%, %count00%, %count000%, %count0000%,
              %counta% and %countA% are counters. Example: Copy (%count%) of
              %name%.
            </p>
            <label class="check">
              <input
                type="checkbox"
                bind:checked={app.options.duplicateStartOne}
              /> Number the first item too
            </label>
            <label class="check">
              <input
                type="checkbox"
                bind:checked={app.options.duplicateAcrossFolders}
              /> Keep names unique across folders
            </label>
          </div>
        {/if}
        <label class="check">
          <input type="checkbox" bind:checked={app.options.skipExisting} /> Skip items
          whose new name already exists
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={app.options.overwrite} /> Replace existing
          items (they go to the trash)
        </label>

        <h3>Linked files</h3>
        <div class="two">
          <label class="sel">
            <span>Main extension</span><input
              bind:value={app.options.linkMaster}
              placeholder="jpg"
            />
          </label>
          <label class="sel">
            <span>Follow with</span><input
              bind:value={app.options.linkLinked}
              placeholder="raw, xmp"
            />
          </label>
        </div>
        <label class="check">
          <input type="checkbox" bind:checked={app.options.linkAcrossFolders} /> Link
          across folders
        </label>
        <p class="muted small">
          Files with the follow extensions get the same new name as the main
          file with the same name.
        </p>

        <h3>Order and numbers</h3>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.retainAutonumber}
            onchange={settingsChanged}
          /> Continue numbering from the last rename
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.reverseOrder}
            onchange={settingsChanged}
          /> Process items in reverse order
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.logActivity}
            onchange={settingsChanged}
          /> Keep a log of renames
        </label>
      </div>
    {:else if tab === "display"}
      <div class="form">
        <div class="two">
          <label class="sel">
            <span>Theme</span>
            <select bind:value={app.settings.theme} onchange={settingsChanged}>
              <option value="system">Same as system</option>
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </select>
          </label>
          <label class="sel">
            <span>Text size</span><input
              type="number"
              min="10"
              max="22"
              bind:value={app.settings.fontSize}
              onchange={settingsChanged}
            />
          </label>
        </div>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.alwaysOnTop}
            onchange={settingsChanged}
          /> Keep the window on top
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.monospaceList}
            onchange={settingsChanged}
          /> Monospaced file list
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.gridlines}
            onchange={settingsChanged}
          /> Grid lines
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.highlightChanges}
            onchange={settingsChanged}
          /> Highlight the changed part of new names
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.showPreviewPane}
            onchange={settingsChanged}
          /> Picture preview
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.autoRefresh}
            onchange={settingsChanged}
          /> Update new names while I type
        </label>
        <h3>Sorting</h3>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.naturalSort}
            onchange={() => (settingsChanged(), app.applySort())}
          /> Natural order (File2 before File10)
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.groupAffected}
            onchange={() => (settingsChanged(), app.applySort())}
          /> Group items that will change
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.foldersWithFiles}
            onchange={() => (settingsChanged(), app.applySort())}
          /> Sort folders with files
        </label>
        <h3>Columns</h3>
        <div class="cols">
          {#each COLUMNS as c (c.id)}
            <label class="check">
              <input
                type="checkbox"
                checked={app.settings.columns.includes(c.id)}
                disabled={c.id === "name"}
                onchange={(e) =>
                  toggleColumn(c.id, (e.target as HTMLInputElement).checked)}
              />
              {c.label}
            </label>
          {/each}
        </div>
        <label class="sel">
          <span>Custom column (tags)</span>
          <input
            class="mono"
            bind:value={app.settings.customColumn}
            onchange={() => (settingsChanged(), app.loadExtraColumns())}
            placeholder="<(exif:Model)> <(Dimensions)>"
          />
        </label>
        <label class="sel">
          <span>File sizes</span>
          <select bind:value={app.settings.sizeUnit} onchange={settingsChanged}>
            <option value="auto">Automatic</option>
            <option value="bytes">Bytes</option>
            <option value="kb">KB</option>
            <option value="mb">MB</option>
          </select>
        </label>
      </div>
    {:else if tab === "general"}
      <div class="form">
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.autoCheckAfterListing}
            onchange={settingsChanged}
          /> Include every item when a folder is listed
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.autoCheckAfterDrop}
            onchange={settingsChanged}
          /> Include dropped and pasted items
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.enterOpensFile}
            onchange={settingsChanged}
          /> Double-click or Enter opens the file (instead of renaming it)
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.recallLastCriteria}
            onchange={settingsChanged}
          /> Start with the last steps I used
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.openLastFolder}
            onchange={settingsChanged}
          /> Open the last folder at start
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.saveRecentEntries}
            onchange={settingsChanged}
          /> Remember recent entries for each field
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.autoload}
            onchange={settingsChanged}
          /> Load _autoload.alterix from a folder when I open or drop it
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.autoSavePreset}
            onchange={settingsChanged}
          /> Save changes to the open preset when I quit
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.storePathInPreset}
            onchange={settingsChanged}
          /> Store the folder in presets
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.allowMultipleInstances}
            onchange={settingsChanged}
          /> Allow more than one Alterix window (after restart)
        </label>
      </div>
    {:else if tab === "messages"}
      <div class="form">
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.warnBeforeRename}
            onchange={settingsChanged}
          /> Ask before renaming
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.confirmEach}
            onchange={settingsChanged}
          /> Ask for each item
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.confirmAfterRename}
            onchange={settingsChanged}
          /> Show a message when renaming is done
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={app.settings.warnRiskyOptions}
            onchange={settingsChanged}
          /> Warn when replace, skip or create-folder options are on
        </label>
      </div>
    {:else if tab === "data"}
      <div class="form">
        <h3>Data folder</h3>
        <p class="small">
          Settings, history, the default preset and the log are kept in:
        </p>
        <p class="path mono small">{app.dataStatus?.path}</p>
        {#if app.dataStatus && !app.dataStatus.writable}
          <p class="bad small">
            Alterix cannot write to this folder. Choose another one for this
            session.
          </p>
        {/if}
        <div class="buttons">
          <button
            onclick={() =>
              app.dataStatus && revealItemInDir(app.dataStatus.path)}
          >
            Show folder
          </button>
          <button onclick={pickDataFolder}>Use another folder…</button>
          <button
            onclick={async () => revealItemInDir(await invoke("log_file"))}
          >
            Show rename log
          </button>
        </div>
        <h3>Help</h3>
        <div class="buttons">
          <button title={fileManagerTip} onclick={() => app.viewLogs()}>
            View Logs
          </button>
          <button onclick={() => app.reportProblem()}>Report a Problem…</button>
        </div>
        <p class="muted small">
          View Logs shows Alterix.log, which helps fix problems. Report a
          Problem opens a bug form on GitHub in your browser with your Alterix
          version and system filled in. You need a free GitHub account. Nothing
          is sent until you submit it there.
        </p>
        <h3>Open in Alterix</h3>
        <p class="small">
          Adds “Open in Alterix” to {navigator.userAgent.includes("Mac")
            ? "Finder (Quick Actions)"
            : navigator.userAgent.includes("Windows")
              ? "the File Explorer context menu"
              : "Nautilus, Dolphin and Open With"}. This writes outside the data
          folder:
        </p>
        {#if shell}
          {#each shell.places as p (p)}<p class="path mono small">{p}</p>{/each}
          <div class="buttons">
            {#if shell.installed}
              <button
                onclick={async () => (shell = await invoke("shell_uninstall"))}
              >
                Remove
              </button>
            {:else}
              <button
                class="primary"
                onclick={async () => {
                  try {
                    shell = await invoke("shell_install");
                  } catch (e) {
                    app.setError(e);
                  }
                }}
              >
                Add
              </button>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
    {#snippet footer()}
      <button class="primary" onclick={() => (settingsChanged(), close())}>
        Done
      </button>
    {/snippet}
  </Modal>
{:else if app.dialog === "history"}
  <Modal title="History" onclose={close} wide>
    {#if !app.history.length}
      <p class="muted">No renames yet.</p>
    {:else}
      <ul class="hist">
        {#each app.history as b (b.id)}
          <li class:undone={b.undone}>
            <div>
              <strong>{b.summary}</strong>
              <span class="muted small">
                {b.when}{b.undone ? " · undone" : ""}
              </span>
              <details>
                <summary class="small">Show items</summary>
                <ul class="ops mono small">
                  {#each b.ops.slice(0, 200) as o, i (i)}<li>
                      {o.from} → {o.to}
                    </li>{/each}
                  {#if b.ops.length > 200}<li>
                      … and {b.ops.length - 200} more
                    </li>{/if}
                </ul>
              </details>
            </div>
            <div class="buttons">
              <button
                onclick={() => saveScript(b.id)}
                title="A script that reverses this rename"
              >
                Undo script…
              </button>
              <button
                class="primary"
                disabled={b.undone}
                onclick={() => app.undo(b.id)}
              >
                Undo
              </button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
    {#snippet footer()}
      <button
        class="quiet"
        onclick={async () => {
          await invoke("clear_history");
          await app.loadHistory();
        }}
      >
        Clear history
      </button>
      <button class="primary" onclick={close}>Done</button>
    {/snippet}
  </Modal>
{:else if app.dialog === "pairs"}
  <Modal title="Rename pairs" onclose={close} wide>
    <p class="small">
      Rename pairs give items a new name from a list: one <code>
        old name|new name
      </code> per line (comma or tab also work). Old names can be full paths. Other
      steps still run after the pair.
    </p>
    <div class="buttons">
      <button onclick={importPairsFile}>Import from file…</button>
      <button
        onclick={async () =>
          app.importPairs((await readText().catch(() => "")) ?? "")}
      >
        Import from clipboard
      </button>
      <button
        disabled={!app.options.pairs.length}
        onclick={() => app.checkPairRows()}
      >
        Include matching items
      </button>
      <button
        disabled={!app.options.pairs.length}
        onclick={() => (app.options.pairs = [])}
      >
        Clear
      </button>
    </div>
    {#if app.options.pairs.length}
      <table class="kv small">
        <thead><tr><th>Old name</th><th>New name</th></tr></thead>
        <tbody>
          {#each app.options.pairs.slice(0, 500) as p, i (i)}<tr>
              <td>{p.find}</td><td>{p.with}</td>
            </tr>{/each}
        </tbody>
      </table>
      {#if app.options.pairs.length > 500}<p class="muted small">
          … and {app.options.pairs.length - 500} more.
        </p>{/if}
    {:else}
      <p class="muted">No pairs imported.</p>
    {/if}
    {#snippet footer()}<button class="primary" onclick={close}>
        Done
      </button>{/snippet}
  </Modal>
{:else if app.dialog === "properties"}
  <Modal title="Properties" onclose={close} wide>
    <p class="path mono small">{app.dialogData}</p>
    {#if propImage}<img class="thumb" src={propImage} alt="" />{/if}
    <input
      class="filter"
      placeholder="Filter properties"
      bind:value={propFilter}
    />
    {#if !props}
      <p class="muted">Reading…</p>
    {:else}
      <table class="kv small">
        <tbody>
          {#each propRows as [k, v] (k)}
            <tr>
              <td class="mono">{k}</td>
              <td>{v}</td>
              <td>
                <button
                  class="mini"
                  title="Copy tag"
                  onclick={() => navigator.clipboard?.writeText(`<(${k})>`)}
                >
                  Tag
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="muted small">
        Use any property in a field as &lt;(name)&gt;, &lt;($name)&gt; for a
        date or &lt;(#name)&gt; for a number.
      </p>
    {/if}
    {#snippet footer()}<button class="primary" onclick={close}>
        Done
      </button>{/snippet}
  </Modal>
{:else if app.dialog === "debug"}
  {@const p = app.dialogData as PreviewItem | null}
  <Modal title="How this name is built" onclose={close}>
    {#if !p}
      <p class="muted">Include this item to see its new name.</p>
    {:else}
      <ol class="trace">
        <li>
          <span class="muted">Start</span><span class="mono">{p.original}</span>
        </li>
        {#each p.trace ?? [] as t, i (i)}<li>
            <span class="muted">{t.step}</span><span class="mono">
              {t.name}
            </span>
          </li>{/each}
        <li><strong>Result</strong><span class="mono">{p.newName}</span></li>
      </ol>
      {#if p.message}<p class="bad small">{p.message}</p>{/if}
      {#if p.newPath !== p.path}<p class="muted small">
          Destination: {p.newPath}
        </p>{/if}
      {#if p.alerts.length}<p class="small">
          Script messages: {p.alerts.join(" · ")}
        </p>{/if}
    {/if}
    {#snippet footer()}<button class="primary" onclick={close}>
        Done
      </button>{/snippet}
  </Modal>
{:else if app.dialog === "report" && app.report}
  <Modal title="Rename report" onclose={close} wide>
    <p>{app.report.renamed} done, {app.report.failed} not done.</p>
    <table class="kv small">
      <tbody>
        {#each app.report.results.filter((r) => !r.ok || r.message) as r (r.path)}
          <tr>
            <td class="mono">{r.path}</td><td class:bad={!r.ok}>{r.message}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#snippet footer()}<button class="primary" onclick={close}>
        Done
      </button>{/snippet}
  </Modal>
{:else if app.dialog === "confirmEach" && confirmItems[confirmIndex]}
  {@const p = confirmItems[confirmIndex]}
  <Modal
    title="Rename item {confirmIndex + 1} of {confirmItems.length}"
    onclose={close}
  >
    <p class="mono">{p.original}</p>
    <p class="muted">becomes</p>
    <p class="mono strong">{p.newName}</p>
    {#snippet footer()}
      <button class="quiet" onclick={() => confirmAnswer("cancel")}>
        Cancel
      </button>
      <button onclick={() => confirmAnswer("no")}>Skip</button>
      <button onclick={() => confirmAnswer("all")}>Yes to all</button>
      <button class="primary" onclick={() => confirmAnswer("yes")}>
        Rename
      </button>
    {/snippet}
  </Modal>
{:else if app.dialog === "jump"}
  <Modal title="Go to folder" onclose={close}>
    <label class="sel">
      <span>Folder path</span><input
        class="mono"
        bind:value={jumpPath}
        onkeydown={(e) =>
          e.key === "Enter" &&
          jumpPath &&
          (close(), app.openFolder(jumpPath.trim()))}
      />
    </label>
    {#if app.settings.lastFolders.length}
      <p class="muted small">Recent folders</p>
      <div class="recent">
        {#each app.settings.lastFolders as f (f)}<button
            class="link mono small"
            onclick={() => (close(), app.openFolder(f))}
          >
            {f}
          </button>{/each}
      </div>
    {/if}
    {#snippet footer()}
      <button class="quiet" onclick={close}>Cancel</button>
      <button
        class="primary"
        disabled={!jumpPath.trim()}
        onclick={() => (close(), app.openFolder(jumpPath.trim()))}
      >
        Open
      </button>
    {/snippet}
  </Modal>
{:else if app.dialog === "manual" && app.dialogData}
  {@const r = app.dialogData as Row}
  {@const go = () => {
    const name = manualName.trim();
    close();
    if (name && name !== r.name) app.manualRename(r.path, name);
  }}
  <Modal title="Rename by hand" onclose={close}>
    <label class="sel">
      <span>New name for {r.name}</span>
      <input
        class="mono"
        bind:value={manualName}
        onkeydown={(e) => {
          if (e.key === "Enter") go();
          if (e.key === "Tab") {
            e.preventDefault();
            const name = manualName.trim();
            const i =
              app.rows.findIndex((x) => x.path === r.path) +
              (e.shiftKey ? -1 : 1);
            if (name && name !== r.name) app.manualRename(r.path, name);
            const next = app.rows[i];
            if (next) {
              app.dialogData = next;
              manualName = next.name;
            } else close();
          }
        }}
      />
    </label>
    <p class="muted small">
      Enter renames. Tab renames and moves to the next item; Shift+Tab to the
      previous one.
    </p>
    {#snippet footer()}
      <button class="quiet" onclick={close}>Cancel</button>
      <button class="primary" onclick={go}>Rename</button>
    {/snippet}
  </Modal>
{:else if app.dialog === "update" && update}
  {@const info = update.error ? null : (update as UpdateInfo)}
  {@const text = info ? summary(info) : null}
  <Modal title="Check for updates" onclose={close}>
    {#if update.error}
      <p>{update.error}</p>
    {:else if info && text}
      <p><strong>{text.headline}</strong></p>
      <p>{text.detail}</p>
      {#if text.offerPage && info.notes}
        <details>
          <summary class="small">Release notes</summary>
          <pre class="notes small">{info.notes}</pre>
        </details>
      {/if}
    {/if}
    {#snippet footer()}
      {#if info && text?.offerPage}
        <button class="quiet" onclick={close}>Close</button>
        <button class="primary" onclick={() => (openUrl(info.url), close())}>
          Open release page
        </button>
      {:else}
        <button class="primary" onclick={close}>Close</button>
      {/if}
    {/snippet}
  </Modal>
{:else if app.dialog === "logs"}
  <Modal title="View Logs" onclose={close}>
    <p>Alterix could not open the file manager. The log file is here:</p>
    <p class="path mono small">{app.dialogData}</p>
    {#snippet footer()}
      <button onclick={() => writeText(String(app.dialogData ?? ""))}>
        Copy Path
      </button>
      <button class="primary" onclick={close}>Close</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--line);
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  .tabs button {
    border: 0;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    background: transparent;
    padding: 6px 10px;
    color: var(--muted);
  }
  .tabs button.active {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .form {
    display: grid;
    gap: 8px;
  }
  h3 {
    margin: 10px 0 0;
    font-size: 12px;
    font-weight: 600;
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .sel {
    display: grid;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
  }
  .indent {
    padding-left: 24px;
    display: grid;
    gap: 6px;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .buttons {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .path {
    margin: 2px 0;
    padding: 6px 8px;
    background: var(--bg);
    border-radius: 6px;
    word-break: break-all;
  }
  .mono {
    font-family: var(--mono);
  }
  .strong {
    font-weight: 600;
  }
  .hist {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .hist > li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  .hist > li.undone {
    opacity: 0.6;
  }
  .hist strong {
    display: block;
  }
  .ops {
    max-height: 200px;
    overflow: auto;
    padding-left: 16px;
  }
  .kv {
    width: 100%;
    border-collapse: collapse;
    margin-top: 8px;
  }
  .kv td,
  .kv th {
    text-align: left;
    padding: 4px 6px;
    border-bottom: 1px solid var(--line-soft);
    vertical-align: top;
    word-break: break-all;
  }
  .filter {
    margin: 8px 0 0;
  }
  .thumb {
    max-width: 100%;
    max-height: 200px;
    border-radius: 6px;
    margin-top: 8px;
  }
  .trace {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 4px;
  }
  .trace li {
    display: grid;
    grid-template-columns: 180px 1fr;
    gap: 8px;
    padding: 4px 0;
    border-bottom: 1px solid var(--line-soft);
  }
  .recent {
    display: grid;
    gap: 2px;
  }
  .link {
    border: 0;
    background: transparent;
    text-align: left;
    padding: 4px;
  }
  .link:hover {
    background: var(--hover);
  }
  .bad {
    color: var(--bad);
  }
  .notes {
    white-space: pre-wrap;
    max-height: 220px;
    overflow: auto;
    background: var(--bg);
    padding: 8px;
    border-radius: 6px;
  }
</style>
