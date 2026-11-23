// Manual update check: asks GitHub for the latest release, only when the user selects
// the version number. Sends one request with no data, downloads and installs nothing.

export const APP_NAME = "Alterix";
export const REPOSITORY = "TariqMK/Alterix";
export const LATEST_URL = `https://api.github.com/repos/${REPOSITORY}/releases/latest`;
export const RELEASES_PAGE = `https://github.com/${REPOSITORY}/releases`;
export const TIMEOUT = 10_000;

/** An error whose message is the finished text shown to the user. */
export class UpdateError extends Error {}

export type Kind = "newer" | "current" | "dev" | "branch";

export interface UpdateInfo {
  current: string;
  latest: string;
  url: string;
  notes: string;
  kind: Kind;
}

/** Parses "v1.2.3" or "1.2.3" into numbers; anything else gives null. */
export function parseVersion(text: string): [number, number, number] | null {
  const m = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(text.trim());
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

/** True only when both parse and `latest` is greater (compared as numbers). */
export function isNewer(latest: string, current: string): boolean {
  const a = parseVersion(latest);
  const b = parseVersion(current);
  if (!a || !b) return false;
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] > b[i];
  return false;
}

const BRANCH = /^v?\d+\.\d+\.\d+-[a-z0-9-]+\.\d+$/;

export function kindOf(current: string, latest: string): Kind {
  if (BRANCH.test(current.trim())) return "branch";
  if (!parseVersion(current)) return "dev";
  return isNewer(latest, current) ? "newer" : "current";
}

/** Builds the result from GitHub's JSON for the latest release. */
export function parseRelease(data: unknown, current: string): UpdateInfo {
  const bad = new UpdateError(
    `GitHub sent an answer ${APP_NAME} can't read. Try again later.`,
  );
  if (!data || typeof data !== "object") throw bad;
  const d = data as Record<string, unknown>;
  const tag = typeof d.tag_name === "string" ? d.tag_name.trim() : "";
  if (!parseVersion(tag)) throw bad;
  const latest = tag.startsWith("v") ? tag : `v${tag}`;
  const url =
    typeof d.html_url === "string" && d.html_url.startsWith("https://")
      ? d.html_url
      : RELEASES_PAGE;
  const notes = typeof d.body === "string" ? d.body.trim() : "";
  return { current, latest, url, notes, kind: kindOf(current, latest) };
}

/** Sends the one request. `fetchImpl` is replaced in tests. */
export async function check(
  current: string,
  fetchImpl: typeof fetch = fetch,
): Promise<UpdateInfo> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), TIMEOUT);
  let res: Response;
  try {
    res = await fetchImpl(LATEST_URL, {
      headers: {
        Accept: "application/vnd.github+json",
        "User-Agent": `${APP_NAME}/${current}`,
      },
      signal: ctrl.signal,
      cache: "no-store",
    });
  } catch (e) {
    const reason = ctrl.signal.aborted
      ? "the request timed out"
      : e instanceof Error
        ? e.message
        : String(e);
    throw new UpdateError(
      `Can't reach GitHub: ${reason}. Check the internet connection.`,
    );
  } finally {
    clearTimeout(timer);
  }
  if (res.status === 404)
    throw new UpdateError(`No ${APP_NAME} releases are published yet.`);
  if (res.status === 403 || res.status === 429)
    throw new UpdateError(
      "GitHub limits how often it can be checked. Try again in an hour.",
    );
  if (!res.ok)
    throw new UpdateError(
      `GitHub answered with error ${res.status}. Try again later.`,
    );
  let data: unknown;
  try {
    data = await res.json();
  } catch {
    throw new UpdateError(
      `GitHub sent an answer ${APP_NAME} can't read. Try again later.`,
    );
  }
  return parseRelease(data, current);
}

/** Headline and detail text for the result dialog. */
export function summary(info: UpdateInfo): {
  headline: string;
  detail: string;
  offerPage: boolean;
} {
  switch (info.kind) {
    case "newer":
      return {
        headline: `${APP_NAME} ${info.latest} is available.`,
        detail: `You have ${info.current}. Open the release page to download the new version.`,
        offerPage: true,
      };
    case "dev":
      return {
        headline: "This is a development build.",
        detail: `You are running ${info.current}. The latest release is ${info.latest}.`,
        offerPage: true,
      };
    case "branch":
      return {
        headline: "This is a test build.",
        detail: `You are running ${info.current}. The latest release is ${info.latest}.`,
        offerPage: true,
      };
    default:
      return {
        headline: `${APP_NAME} is up to date.`,
        detail: `${info.current} is the latest release.`,
        offerPage: false,
      };
  }
}
