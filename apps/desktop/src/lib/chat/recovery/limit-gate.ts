/** Limits that need different recovery advice or reset behavior in the chat composer. */
export type LimitGateKind = 'five-hour' | 'daily' | 'seven-day' | 'thirty-day' | 'requests' | 'credit' | 'plan' | 'generic';

export type LimitGate = {
  kind: LimitGateKind;
  label: string;
  detail: string;
  /** Only limits with a known reset should be retried automatically. Credit and plan gates need user action. */
  canAutoResume: boolean;
};

/** Classifies a provider's limit error and decides whether its reset is safe to wait for. */
export function classifyLimitGate(message: string, resetAt: number | null, model?: string): LimitGate {
  const text = message.toLowerCase();
  let kind: LimitGateKind = 'generic';

  if (/5\s*[- ]?hour|five[- ]hour|5h\b/.test(text)) kind = 'five-hour';
  else if (/30\s*[- ]?day|monthly|month(?:ly)? limit/.test(text)) kind = 'thirty-day';
  else if (/7\s*[- ]?day|weekly|week(?:ly)? limit/.test(text)) kind = 'seven-day';
  else if (/daily|per[- ]day|day limit/.test(text)) kind = 'daily';
  else if (/request(?:s)?\s+(?:limit|quota|cap)|too many requests|requests? per|\b429\b|rate.?limit/.test(text)) kind = 'requests';
  else if (/credit balance|out of credits|insufficient[_ ](?:credit|quota)|no credits|credit.*(?:exhausted|depleted|empty|too low)|(?:exhausted|depleted|empty).*credit|billing.*(?:credit|balance)|payment required/.test(text)) kind = 'credit';
  else if (/upgrade (?:your )?plan|plan limit|subscription limit|buy more|purchase more/.test(text)) kind = 'plan';
  else if (kind === 'generic' && model?.startsWith('credit:')) kind = 'credit';

  const creditReset = /credit.{0,30}(?:resets?|renew)|(?:resets?|renew).{0,30}credit/i.test(message);
  const planReset = /plan.{0,30}(?:resets?|renew)|(?:resets?|renew).{0,30}plan/i.test(message);
  const hasReset = resetAt !== null && resetAt > Date.now();
  const canAutoResume = hasReset && (kind === 'credit' ? creditReset : kind === 'plan' ? planReset : true);
  const label = kind === 'five-hour' ? '5-hour limit reached'
    : kind === 'daily' ? 'Daily limit reached'
      : kind === 'seven-day' ? '7-day limit reached'
        : kind === 'thirty-day' ? '30-day limit reached'
          : kind === 'requests' ? 'Request limit reached'
            : kind === 'credit' ? 'Credit exhausted'
              : kind === 'plan' ? 'Plan limit reached'
                : 'Usage limit reached';
  const detail = kind === 'credit' ? 'Add credit or choose another model'
    : kind === 'plan' ? 'Upgrade your plan or choose another model'
      : kind === 'requests' ? 'Request quota reached'
        : 'Try again after the provider limit resets';

  return { kind, label, detail, canAutoResume };
}
