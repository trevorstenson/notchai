interface ModelPricing {
  inputPerMillion: number;
  outputPerMillion: number;
}

const PRICING: Record<string, ModelPricing> = {
  opus: { inputPerMillion: 15, outputPerMillion: 75 },
  sonnet: { inputPerMillion: 3, outputPerMillion: 15 },
  haiku: { inputPerMillion: 0.8, outputPerMillion: 4 },
};

// Default to Sonnet when model is unknown — most common model.
const DEFAULT_PRICING: ModelPricing = PRICING.sonnet;

function getModelPricing(model: string | null): ModelPricing {
  if (!model) return DEFAULT_PRICING;
  const lower = model.toLowerCase();
  if (lower.includes("opus")) return PRICING.opus;
  if (lower.includes("sonnet")) return PRICING.sonnet;
  if (lower.includes("haiku")) return PRICING.haiku;
  return DEFAULT_PRICING;
}

/**
 * Calculate cost in USD. Note: v1 slightly overestimates because
 * cache_read tokens are lumped into inputTokens by the backend.
 */
export function calculateSessionCost(
  inputTokens: number,
  outputTokens: number,
  model: string | null,
): number {
  const pricing = getModelPricing(model);
  const inputCost = (inputTokens / 1_000_000) * pricing.inputPerMillion;
  const outputCost = (outputTokens / 1_000_000) * pricing.outputPerMillion;
  return inputCost + outputCost;
}

export function formatCost(usd: number): string {
  if (usd <= 0) return "$0";
  if (usd < 0.01) return "<$0.01";
  if (usd < 10) return `$${usd.toFixed(2)}`;
  if (usd < 100) return `$${usd.toFixed(1)}`;
  return `$${Math.round(usd)}`;
}
