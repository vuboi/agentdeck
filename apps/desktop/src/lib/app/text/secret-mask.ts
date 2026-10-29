// How a saved key reads once it is saved: a few characters at each end, so two keys can be told apart, and the rest
// hidden. Free of stores so plain node can test it.

/** `sk-ant-api03-…abcd` reads `sk-ant…••••••••abcd`; a short key is hidden whole. */
export function maskSecret(secret: string): string {
  const key = secret.trim();
  if (key.length < 16) return '••••••••';
  return `${key.slice(0, 6)}…••••••••${key.slice(-4)}`;
}
