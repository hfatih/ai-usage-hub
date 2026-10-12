import { expect, it } from "vitest";
import { byProviderOrder } from "./providers";

it("puts Google first, groups providers together and keeps order inside a group", () => {
  const ids = ["openai:a", "google:a", "mystery:a", "anthropic:a", "google:b", "kimi:a", "opencode:a", "gemini_cli:a"];
  const sorted = byProviderOrder(ids, (id) => id.split(":")[0]);
  expect(sorted).toEqual(["google:a", "google:b", "gemini_cli:a", "anthropic:a", "openai:a", "opencode:a", "kimi:a", "mystery:a"]);
});
