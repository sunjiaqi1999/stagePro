export interface AppConfig {
  scanDirs: string[]
  emails: string[]
  autostart: boolean
  targetBranch: string
  dateRangeStart: string
  dateRangeEnd: string
  aiModels: AiModelConfig[]
  defaultAiModelId: string
  aiSkills: AiSkillConfig[]
}

export interface AiModelConfig {
  id: string
  name: string
  provider: string
  model: string
  baseUrl: string
  apiKey: string
}

export type AiSkillContextPolicy = 'none' | 'optional-git' | 'required-git'

export interface AiSkillConfig {
  id: string
  label: string
  description: string
  systemPrompt: string
  taskPrompt: string
  outputFormat: string
  contextPolicy: AiSkillContextPolicy
  temperature: number
}

export interface Summary {
  totalCommits: number
  totalAdditions: number
  totalDeletions: number
  repoCount: number
  mergedBranchCount: number
}

export interface CommitFileChange {
  path: string
  additions: number | null
  deletions: number | null
}

export interface CommitRecord {
  repoName: string
  repoPath: string
  hash: string
  authorName: string
  authorEmail: string
  authorDate: string
  subject: string
  additions: number | null
  deletions: number | null
  files: CommitFileChange[]
}

export interface MergedBranchRecord {
  repoName: string
  repoPath: string
  targetBranch: string
  targetRef: string
  sourceBranch: string | null
  hash: string
  committerName: string
  committerEmail: string
  mergedAt: string
  subject: string
}

export interface ScanIssue {
  path?: string | null
  message: string
}

export interface WeeklyReport {
  summary: Summary
  commits: CommitRecord[]
  mergedBranches: MergedBranchRecord[]
  issues: ScanIssue[]
  scannedAt: string
  weekStart: string
  weekEnd: string
  targetBranch: string
}
