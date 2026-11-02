export const ONBOARDING_GUIDE_STEP_IDS = ['welcome', 'map', 'path', 'tips', 'go'] as const;

export type OnboardingGuideStepId = (typeof ONBOARDING_GUIDE_STEP_IDS)[number];
