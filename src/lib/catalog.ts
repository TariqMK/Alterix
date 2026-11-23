// The catalogue of rename steps: labels, defaults and the field layout the generic
// step editor draws. Field keys and values match the Rust engine's JSON names.

export type RuleKind =
  | "replace"
  | "regex"
  | "remove"
  | "add"
  | "case"
  | "name"
  | "moveCopy"
  | "translate"
  | "numbering"
  | "autoDate"
  | "folderName"
  | "extension"
  | "segment"
  | "script";

export type Rule = { kind: RuleKind } & Record<string, unknown>;

export interface Step {
  id: number;
  enabled: boolean;
  collapsed: boolean;
  rule: Rule;
}

type Opt = [string, string];

export type Field =
  | {
      type: "text";
      key: string;
      label: string;
      tags?: boolean;
      placeholder?: string;
      mono?: boolean;
      show?: (r: Rule) => boolean;
      help?: string;
    }
  | {
      type: "area";
      key: string;
      label: string;
      tags?: boolean;
      placeholder?: string;
      rows?: number;
      show?: (r: Rule) => boolean;
      help?: string;
    }
  | {
      type: "number";
      key: string;
      label: string;
      min?: number;
      max?: number;
      step?: number;
      show?: (r: Rule) => boolean;
      help?: string;
    }
  | {
      type: "check";
      key: string;
      label: string;
      show?: (r: Rule) => boolean;
      help?: string;
    }
  | {
      type: "select";
      key: string;
      label: string;
      options: Opt[];
      show?: (r: Rule) => boolean;
      help?: string;
    }
  | {
      type: "pairs";
      key: string;
      label: string;
      show?: (r: Rule) => boolean;
      help?: string;
    }
  | { type: "row"; fields: Field[]; show?: (r: Rule) => boolean }
  | { type: "note"; text: string; show?: (r: Rule) => boolean };

export interface RuleDef {
  kind: RuleKind;
  label: string;
  summary: string;
  group: string;
  defaults: () => Rule;
  fields: Field[];
}

const POSITION: Opt[] = [
  ["prefix", "Before the name"],
  ["suffix", "After the name"],
];

export const RULES: RuleDef[] = [
  {
    kind: "replace",
    label: "Replace text",
    summary:
      "Swap text for other text. Supports | lists, * ? wildcards and \\first\\ … \\last\\.",
    group: "Text",
    defaults: () => ({
      kind: "replace",
      pairs: [{ find: "", with: "" }],
      matchCase: false,
    }),
    fields: [
      {
        type: "pairs",
        key: "pairs",
        label: "Replacements",
        help: "Use a|b to replace several values. Start with \\regex\\ for a regular expression, or \\first\\, \\last\\, \\start\\, \\end\\, \\second\\ … to pick one match.",
      },
      { type: "check", key: "matchCase", label: "Match case" },
    ],
  },
  {
    kind: "regex",
    label: "Regular expression",
    summary: "Match and replace with a regular expression.",
    group: "Text",
    defaults: () => ({
      kind: "regex",
      pattern: "",
      replace: "",
      includeExt: false,
      mode: "v2",
    }),
    fields: [
      {
        type: "text",
        key: "pattern",
        label: "Match",
        mono: true,
        placeholder: "(\\d+)-(.*)",
      },
      {
        type: "text",
        key: "replace",
        label: "Replace with",
        mono: true,
        tags: true,
        placeholder: "$2 $1",
      },
      {
        type: "row",
        fields: [
          {
            type: "select",
            key: "mode",
            label: "Mode",
            options: [
              ["v2", "Modern (keeps unmatched text)"],
              ["standard", "Classic (match replaces the name)"],
              ["simple", "Simple (%1 … %9)"],
            ],
          },
          { type: "check", key: "includeExt", label: "Include extension" },
        ],
      },
      {
        type: "note",
        text: "Join several expressions with (?X). Add /i or /g at the end in Modern mode. Use \\U \\L \\E \\u \\l to change case in the replacement.",
      },
    ],
  },
  {
    kind: "remove",
    label: "Remove",
    summary: "Remove characters, words, ranges or whole classes of characters.",
    group: "Text",
    defaults: () => ({
      kind: "remove",
      first: 0,
      last: 0,
      from: 0,
      to: 0,
      chars: "",
      words: "",
      crop: "none",
      cropText: "",
      digits: false,
      high: false,
      trim: false,
      doubleSpaces: false,
      accents: false,
      allChars: false,
      symbols: false,
      leadDots: "none",
    }),
    fields: [
      {
        type: "row",
        fields: [
          { type: "number", key: "first", label: "First n", min: 0 },
          { type: "number", key: "last", label: "Last n", min: 0 },
          {
            type: "number",
            key: "from",
            label: "From",
            help: "Position; negative counts from the end",
          },
          { type: "number", key: "to", label: "To" },
        ],
      },
      {
        type: "row",
        fields: [
          {
            type: "text",
            key: "chars",
            label: "Characters",
            placeholder: "QW:# or >c<Z",
          },
          {
            type: "text",
            key: "words",
            label: "Words",
            placeholder: "copy final",
          },
        ],
      },
      {
        type: "row",
        fields: [
          {
            type: "select",
            key: "crop",
            label: "Crop",
            options: [
              ["none", "No crop"],
              ["before", "Before text"],
              ["after", "After text"],
              ["special", "Pattern with *"],
            ],
          },
          {
            type: "text",
            key: "cropText",
            label: "Crop text",
            placeholder: "[*]",
            show: (r) => r.crop !== "none",
          },
        ],
      },
      {
        type: "row",
        fields: [
          { type: "check", key: "digits", label: "Digits" },
          { type: "check", key: "symbols", label: "Symbols" },
          { type: "check", key: "accents", label: "Accents" },
          { type: "check", key: "allChars", label: "Letters" },
          { type: "check", key: "high", label: "High (128–255)" },
          { type: "check", key: "trim", label: "Trim" },
          { type: "check", key: "doubleSpaces", label: "Double spaces" },
        ],
      },
      {
        type: "select",
        key: "leadDots",
        label: "Leading dots",
        options: [
          ["none", "Keep"],
          ["single", "Remove one ."],
          ["double", "Remove .."],
          ["all", "Remove all"],
        ],
      },
      {
        type: "note",
        text: "Removed text is available to later steps as <removed>.",
      },
    ],
  },
  {
    kind: "add",
    label: "Add",
    summary: "Add a prefix, a suffix or text at a position.",
    group: "Text",
    defaults: () => ({
      kind: "add",
      prefix: "",
      insert: "",
      at: 0,
      suffix: "",
      wordSpace: false,
    }),
    fields: [
      { type: "text", key: "prefix", label: "Prefix", tags: true },
      {
        type: "row",
        fields: [
          { type: "text", key: "insert", label: "Insert", tags: true },
          {
            type: "number",
            key: "at",
            label: "At position",
            help: "Negative counts from the end",
          },
        ],
      },
      { type: "text", key: "suffix", label: "Suffix", tags: true },
      {
        type: "check",
        key: "wordSpace",
        label: "Space before capitals (MyFile → My File)",
      },
    ],
  },
  {
    kind: "case",
    label: "Change case",
    summary: "Lower, upper, title or sentence case, with exceptions.",
    group: "Text",
    defaults: () => ({ kind: "case", mode: "title", exceptions: "" }),
    fields: [
      {
        type: "select",
        key: "mode",
        label: "Style",
        options: [
          ["title", "Title Case"],
          ["titleEnhanced", "Title Case (smart small words)"],
          ["sentence", "Sentence case"],
          ["lower", "lower case"],
          ["upper", "UPPER CASE"],
          ["same", "Keep (exceptions only)"],
        ],
      },
      {
        type: "text",
        key: "exceptions",
        label: "Exceptions",
        placeholder: "PHP:ASP:$between:<rnup>",
        help: "Colon separated. $word keeps a word small, $WORD keeps it upper, <ic> ignores ALL CAPS words, <clear> drops the default small words, <rnup>/<rnlo> set Roman numerals.",
      },
    ],
  },
  {
    kind: "name",
    label: "Name",
    summary:
      "Replace, remove or reverse the name, pad numbers or reformat dates.",
    group: "Text",
    defaults: () => ({ kind: "name", mode: "fixed", value: "" }),
    fields: [
      {
        type: "select",
        key: "mode",
        label: "Action",
        options: [
          ["fixed", "Set a new name"],
          ["remove", "Remove the name"],
          ["reverse", "Reverse the name"],
          ["padNumbers", "Pad the first number"],
          ["reformatDate", "Reformat a date"],
        ],
      },
      {
        type: "text",
        key: "value",
        label: "New name",
        tags: true,
        show: (r) => r.mode === "fixed",
        placeholder: "<name{upper}>",
      },
      {
        type: "text",
        key: "value",
        label: "Width",
        show: (r) => r.mode === "padNumbers",
        placeholder: "4 or 4>_",
      },
      {
        type: "text",
        key: "value",
        label: "Date format",
        show: (r) => r.mode === "reformatDate",
        placeholder: "%Y-%m-%d or %d+%m+%Y>%d %m %Y",
      },
    ],
  },
  {
    kind: "moveCopy",
    label: "Move or copy part",
    summary: "Move or copy characters to the start, the end or a position.",
    group: "Text",
    defaults: () => ({
      kind: "moveCopy",
      action: "moveLast",
      count: 1,
      from: 1,
      to: 1,
      target: "start",
      position: 1,
      separator: "",
    }),
    fields: [
      {
        type: "row",
        fields: [
          {
            type: "select",
            key: "action",
            label: "Action",
            options: [
              ["moveFirst", "Move first n"],
              ["moveLast", "Move last n"],
              ["copyFirst", "Copy first n"],
              ["copyLast", "Copy last n"],
              ["moveRange", "Move range"],
              ["copyRange", "Copy range"],
            ],
          },
          {
            type: "number",
            key: "count",
            label: "Characters",
            min: 0,
            show: (r) => !String(r.action).endsWith("Range"),
          },
          {
            type: "number",
            key: "from",
            label: "From",
            show: (r) => String(r.action).endsWith("Range"),
          },
          {
            type: "number",
            key: "to",
            label: "To",
            show: (r) => String(r.action).endsWith("Range"),
          },
        ],
      },
      {
        type: "row",
        fields: [
          {
            type: "select",
            key: "target",
            label: "To",
            options: [
              ["start", "Start"],
              ["end", "End"],
              ["position", "Position"],
            ],
          },
          {
            type: "number",
            key: "position",
            label: "Position",
            show: (r) => r.target === "position",
          },
          { type: "text", key: "separator", label: "Separator" },
        ],
      },
    ],
  },
  {
    kind: "translate",
    label: "Character translations",
    summary: "Translate characters or sequences, e.g. $=DOLLAR.",
    group: "Text",
    defaults: () => ({ kind: "translate", rules: "" }),
    fields: [
      {
        type: "area",
        key: "rules",
        label: "Translations (one per line)",
        rows: 4,
        placeholder: "A=B\n41=42\n$=D,O,L,L,A,R",
      },
      {
        type: "note",
        text: "1 character = itself, 2 characters = hex, 3 characters = decimal. Separate a sequence with commas.",
      },
    ],
  },
  {
    kind: "numbering",
    label: "Numbers",
    summary: "Add sequential numbers in any base, letters or Roman numerals.",
    group: "Numbers and dates",
    defaults: () => ({
      kind: "numbering",
      position: "prefix",
      at: 1,
      start: 1,
      step: 1,
      pad: 2,
      separator: " ",
      breakAt: 0,
      perFolder: false,
      numType: "decimal",
      base: 16,
      letterCase: "default",
    }),
    fields: [
      {
        type: "row",
        fields: [
          {
            type: "select",
            key: "position",
            label: "Position",
            options: [
              ["prefix", "Before the name"],
              ["suffix", "After the name"],
              ["both", "Both ends"],
              ["insert", "At position"],
            ],
          },
          {
            type: "number",
            key: "at",
            label: "At",
            show: (r) => r.position === "insert",
          },
          {
            type: "text",
            key: "separator",
            label: "Separator",
            help: "A : is replaced by the number, e.g. (:)",
          },
        ],
      },
      {
        type: "row",
        fields: [
          { type: "number", key: "start", label: "Start" },
          { type: "number", key: "step", label: "Step" },
          { type: "number", key: "pad", label: "Digits", min: 0, max: 20 },
        ],
      },
      {
        type: "row",
        fields: [
          {
            type: "select",
            key: "numType",
            label: "Type",
            options: [
              ["decimal", "1, 2, 3"],
              ["base", "Base 2–36"],
              ["lettersUpper", "A, B, C"],
              ["lettersLower", "a, b, c"],
              ["roman", "I, II, III"],
              ["hebrew", "Hebrew"],
            ],
          },
          {
            type: "number",
            key: "base",
            label: "Base",
            min: 2,
            max: 36,
            show: (r) => r.numType === "base",
          },
          {
            type: "select",
            key: "letterCase",
            label: "Case",
            options: [
              ["default", "Default"],
              ["upper", "Upper"],
              ["lower", "Lower"],
            ],
            show: (r) => ["base", "roman"].includes(String(r.numType)),
          },
        ],
      },
      {
        type: "row",
        fields: [
          {
            type: "number",
            key: "breakAt",
            label: "Restart at character",
            min: 0,
            help: "Start again from Start when this character of the name changes (0 = off)",
          },
          { type: "check", key: "perFolder", label: "Restart in each folder" },
        ],
      },
    ],
  },
  {
    kind: "autoDate",
    label: "Date",
    summary: "Add a file date, the date taken or today's date.",
    group: "Numbers and dates",
    defaults: () => ({
      kind: "autoDate",
      position: "prefix",
      source: "modified",
      preset: "ymd",
      custom: "",
      separator: " ",
      segment: "-",
      century: true,
      offsetHours: 0,
    }),
    fields: [
      {
        type: "row",
        fields: [
          {
            type: "select",
            key: "position",
            label: "Position",
            options: POSITION,
          },
          {
            type: "select",
            key: "source",
            label: "Date",
            options: [
              ["modified", "Modified"],
              ["created", "Created"],
              ["accessed", "Accessed"],
              ["current", "Today"],
              ["takenOriginal", "Taken (original)"],
              ["takenDigitized", "Taken (digitised)"],
              ["takenModified", "Taken (edited)"],
              ["takenRecent", "Taken (most recent)"],
              ["itemDate", "Item date (best available)"],
              ["createdNew", "Created (new)"],
              ["modifiedNew", "Modified (new)"],
            ],
          },
        ],
      },
      {
        type: "row",
        fields: [
          {
            type: "select",
            key: "preset",
            label: "Format",
            options: [
              ["ymd", "Year Month Day"],
              ["ymdHm", "Y M D H M"],
              ["ymdHms", "Y M D H M S"],
              ["dmy", "Day Month Year"],
              ["dmyHm", "D M Y H M"],
              ["dmyHms", "D M Y H M S"],
              ["mdy", "Month Day Year"],
              ["mdyHm", "M D Y H M"],
              ["mdyHms", "M D Y H M S"],
              ["custom", "Custom"],
            ],
          },
          {
            type: "text",
            key: "segment",
            label: "Between parts",
            show: (r) => r.preset !== "custom",
          },
          {
            type: "check",
            key: "century",
            label: "4-digit year",
            show: (r) => r.preset !== "custom",
          },
        ],
      },
      {
        type: "text",
        key: "custom",
        label: "Custom format",
        mono: true,
        placeholder: "%Y-%m-%d or {YYYYMMDD_HHmmss}",
        show: (r) => r.preset === "custom",
        help: "% codes (%Y %m %d %H %M %S %a %b …) or {YYYY-MM-DD}. Start with |U or |L for upper or lower case.",
      },
      {
        type: "row",
        fields: [
          { type: "text", key: "separator", label: "Separator" },
          {
            type: "number",
            key: "offsetHours",
            label: "Offset (hours)",
            step: 0.5,
          },
        ],
      },
    ],
  },
  {
    kind: "folderName",
    label: "Folder name",
    summary: "Add the name of the containing folder or folders.",
    group: "Numbers and dates",
    defaults: () => ({
      kind: "folderName",
      position: "prefix",
      separator: " - ",
      levels: 1,
    }),
    fields: [
      {
        type: "row",
        fields: [
          {
            type: "select",
            key: "position",
            label: "Position",
            options: POSITION,
          },
          { type: "text", key: "separator", label: "Separator" },
          {
            type: "number",
            key: "levels",
            label: "Levels",
            help: "2 adds two folders; -2 adds only the second folder up",
          },
        ],
      },
    ],
  },
  {
    kind: "extension",
    label: "Extension",
    summary:
      "Change the case of the extension, replace it, add one or remove it.",
    group: "Extension",
    defaults: () => ({ kind: "extension", mode: "lower", value: "" }),
    fields: [
      {
        type: "row",
        fields: [
          {
            type: "select",
            key: "mode",
            label: "Action",
            options: [
              ["lower", "lower case"],
              ["upper", "UPPER CASE"],
              ["title", "Title case"],
              ["fixed", "Replace with"],
              ["extra", "Add an extension"],
              ["remove", "Remove"],
              ["same", "Keep"],
            ],
          },
          {
            type: "text",
            key: "value",
            label: "Extension",
            placeholder: "jpg",
            show: (r) => r.mode === "fixed" || r.mode === "extra",
          },
        ],
      },
    ],
  },
  {
    kind: "segment",
    label: "Name segment",
    summary: "Make the following steps change only part of the name.",
    group: "Advanced",
    defaults: () => ({ kind: "segment", from: "1", to: "5" }),
    fields: [
      {
        type: "row",
        fields: [
          {
            type: "text",
            key: "from",
            label: "From",
            placeholder: "1, -5, ( or _,2",
          },
          {
            type: "text",
            key: "to",
            label: "To",
            placeholder: "5, -2, ) or _,3",
          },
        ],
      },
      {
        type: "note",
        text: "Positions (negative from the end) or text with an optional occurrence. Leave both empty to go back to the whole name.",
      },
    ],
  },
  {
    kind: "script",
    label: "Script",
    summary:
      "Full control with JavaScript (newName, object(), exif(), fileProperty() …).",
    group: "Advanced",
    defaults: () => ({
      kind: "script",
      code: "// name, ext, counter, object('modified') …\nnewName = name;\n",
    }),
    fields: [{ type: "area", key: "code", label: "JavaScript", rows: 8 }],
  },
];

export const RULE_BY_KIND: Record<RuleKind, RuleDef> = Object.fromEntries(
  RULES.map((r) => [r.kind, r]),
) as Record<RuleKind, RuleDef>;

/** Tags offered by the tag picker, grouped. */
export const TAG_GROUPS: { group: string; tags: [string, string][] }[] = [
  {
    group: "File",
    tags: [
      ["<name>", "Current name"],
      ["<ext>", "Extension"],
      ["<folder>", "Folder name"],
      ["<counter>", "Counter (1, 2, 3 …)"],
      ["<size>", "Size in bytes"],
      ["<numfiles>", "Files in the folder"],
      ["<removed>", "Text removed by Remove"],
      ["<clip>", "Clipboard text"],
    ],
  },
  {
    group: "Dates",
    tags: [
      ["<($System.ItemDate){YYYY-MM-DD}>", "Item date"],
      ["<($exif:DateTimeOriginal){YYYYMMDD_hhmmss}>", "Date taken"],
      ["<($System.DateModified){YYYY-MM-DD}>", "Modified"],
      ["<($System.DateCreated){YYYY-MM-DD}>", "Created"],
    ],
  },
  {
    group: "Photo",
    tags: [
      ["<(exif:Make)>", "Camera make"],
      ["<(exif:Model)>", "Camera model"],
      ["<(Dimensions)>", "Dimensions"],
      ["<(#exif:FNumber)>", "Aperture"],
      ["<(exif:ExposureTime)>", "Exposure"],
      ["<(#ISO speed)>", "ISO"],
      ["<(exif:GeoLocation.Latitude)>", "Latitude"],
    ],
  },
  {
    group: "Music and video",
    tags: [
      ["<(Artist)>", "Artist"],
      ["<(Album)>", "Album"],
      ["<(Title)>", "Title"],
      ["<(#Track){pad0:2}>", "Track number"],
      ["<(Year)>", "Year"],
      ["<(Duration)>", "Duration"],
      ["<(MediaInfo:FrameRate){fixed:2}>", "Frame rate"],
    ],
  },
  {
    group: "Documents",
    tags: [
      ["<(PDF:Title)>", "PDF title"],
      ["<(PDF:Author)>", "PDF author"],
      ["<(PDF:PageCount)>", "Pages"],
    ],
  },
  {
    group: "Unique",
    tags: [
      ["<(hash:crc32)>", "CRC32"],
      ["<(hash:md5)>", "MD5"],
      ["<(hash:sha256){substr:1:8}>", "SHA-256 (8 chars)"],
      ["<uuid>", "UUID"],
      ["<ulid>", "ULID"],
      ["<rand:6>", "Random letters and digits"],
      ["<randnum:6>", "Random digits"],
    ],
  },
];

/** Prompts used by askOnce('…') in a script, so the UI can ask for answers. */
export function scriptPrompts(code: string): string[] {
  const out: string[] = [];
  const re = /askOnce\(\s*(['"`])((?:\\.|(?!\1).)*)\1/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(code))) {
    if (!out.includes(m[2])) out.push(m[2]);
  }
  return out;
}

/** True when a step reads the clipboard. */
export function usesClipboard(r: Rule): boolean {
  return (
    JSON.stringify(r).includes("<clip") ||
    (r.kind === "script" && String(r.code).includes("clipboard()"))
  );
}
