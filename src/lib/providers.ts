/** Display order wherever provider cards or accounts are listed; unknown providers go last. */
export const PROVIDER_ORDER = ["google", "gemini_cli", "anthropic", "openai", "opencode", "kimi", "github", "cursor", "grok", "deepseek"];

/** Letters shown in a provider's round logo. */
export const PROVIDER_INITIALS: Record<string, string> = { openai: "O", anthropic: "A", google: "G", gemini_cli: "GC", opencode: "OC", kimi: "K", github: "GH", cursor: "CU", grok: "X", deepseek: "DS" };

function rank(providerId: string): number {
  const index = PROVIDER_ORDER.indexOf(providerId);
  return index < 0 ? PROVIDER_ORDER.length : index;
}

/** Sorts by provider order; the incoming order is kept inside each provider. */
export function byProviderOrder<T>(items: T[], providerIdOf: (item: T) => string): T[] {
  return [...items].sort((a, b) => rank(providerIdOf(a)) - rank(providerIdOf(b)));
}
