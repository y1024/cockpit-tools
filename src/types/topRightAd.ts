export interface TopRightAd {
  id: string;
  priority: number;
  title: string;
  summary: string;
  badge?: string | null;
  ctaLabel?: string | null;
  ctaUrl?: string | null;
  targetVersions: string;
  targetLanguages?: string[];
  createdAt: string;
  expiresAt?: string | null;
}

export interface TopRightAdState {
  ad: TopRightAd | null;
}
