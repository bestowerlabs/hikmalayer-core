// Defences for strings that come from the chain rather than from us.
//
// Anyone can issue a token and choose its symbol and name. React escapes HTML,
// so these are not an injection vector — but they ARE a spoofing vector:
// bidirectional overrides can visually reverse text, zero-width characters can
// hide content, and a token can simply call itself "HKM" to impersonate the
// native coin. A user tricked about *which* asset they are handling will sign
// a perfectly valid transaction that does the wrong thing.

// Ranges that can hide or re-order rendered text. Declared as code points so
// no invisible character is ever embedded in this source file.
const HIDDEN_RANGES = [
  [0x0000, 0x001f], // C0 controls
  [0x007f, 0x009f], // DEL + C1 controls
  [0x00ad, 0x00ad], // soft hyphen
  [0x200b, 0x200f], // zero-width space/joiners, LRM/RLM
  [0x202a, 0x202e], // bidi embeddings and overrides
  [0x2060, 0x2064], // word joiner, invisible operators
  [0x2066, 0x2069], // bidi isolates
  [0xfeff, 0xfeff], // BOM / zero-width no-break space
];

const HIDDEN_PATTERN = new RegExp(
  `[${HIDDEN_RANGES.map(
    ([lo, hi]) =>
      `\\u${lo.toString(16).padStart(4, "0")}-\\u${hi.toString(16).padStart(4, "0")}`
  ).join("")}]`,
  "g"
);

/// Make an untrusted string safe to display: strip invisible/reordering
/// characters, collapse whitespace, and bound the length.
export function safeText(value, max = 48) {
  const cleaned = String(value ?? "")
    .replace(HIDDEN_PATTERN, "")
    .replace(/\s+/g, " ")
    .trim();
  if (!cleaned) return "";
  return cleaned.length > max ? `${cleaned.slice(0, max)}…` : cleaned;
}

/// True when the string contains characters we strip — the UI surfaces this
/// so a deliberately deceptive token is visible as such.
export function hadHiddenCharacters(value) {
  HIDDEN_PATTERN.lastIndex = 0; // the pattern is global; reset before testing
  return HIDDEN_PATTERN.test(String(value ?? ""));
}

/// A native-token symbol may never be presented as the chain's own asset.
/// Case- and confusable-insensitive on the obvious substitutions.
export function impersonatesNativeCoin(symbol) {
  const normalized = safeText(symbol, 32)
    .toUpperCase()
    .replace(/0/g, "O")
    .replace(/[1|!]/g, "I")
    .replace(/5/g, "S")
    .replace(/[^A-Z]/g, "");
  return normalized === "HKM" || normalized === "HIKMA" || normalized === "HIKMALAYER";
}

/// Everything a UI needs to render an untrusted asset honestly.
export function describeAsset(asset) {
  const symbol = safeText(asset?.symbol, 12);
  const name = safeText(asset?.name, 48);
  const imitatesNative = impersonatesNativeCoin(asset?.symbol);
  const hidden =
    hadHiddenCharacters(asset?.symbol) || hadHiddenCharacters(asset?.name);
  return {
    symbol: symbol || "(unnamed)",
    name,
    suspicious: imitatesNative || hidden,
    warning: imitatesNative
      ? "This token imitates the native coin HKM. It is NOT HKM."
      : hidden
      ? "This token's name contains hidden or text-reordering characters."
      : null,
  };
}
