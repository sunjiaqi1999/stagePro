<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart'
import {
  ArrowLeft,
  BarChart3,
  Bot,
  Calendar,
  CalendarDays,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Copy,
  FileText,
  FolderPlus,
  GitBranch,
  Loader2,
  Mail,
  MoreVertical,
  PencilLine,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  Send,
  Settings,
  Trash2,
  X,
} from 'lucide-vue-next'
import { renderMarkdownToHtml } from './markdown'
import type { AiModelConfig, AiSkillConfig, AiSkillContextPolicy, AppConfig, CommitFileChange, CommitRecord, MergedBranchRecord, WeeklyReport } from './types'

const DEFAULT_TARGET_BRANCH = 'main'
const DEEPSEEK_MODEL_ID = 'deepseek-chat'
const ALL_PROJECTS = ''
const MAX_PROJECT_SUMMARY_ITEMS = 4
const MAX_AI_COMMIT_CONTEXT = 12
const MAX_AI_FILES_PER_COMMIT = 8
const MAX_COMMIT_FILE_CHIPS = 4
const MAX_AI_PROJECT_SUMMARY_PROJECTS = 12
const MAX_AI_PROJECT_COMMITS = 10
const MAX_AI_PROJECT_MERGES = 4
const CUSTOM_QUICK_RANGE = 'custom'
const BROWSER_CONFIG_STORAGE_KEY = 'gitsage-preview-config'

const QUICK_RANGE_OPTIONS = [
  { value: 'this-week', label: '本周' },
  { value: 'this-month', label: '本月' },
  { value: 'today', label: '近一天' },
  { value: 'last-3-days', label: '近3天' },
  { value: 'last-7-days', label: '近一周' },
  { value: 'last-14-days', label: '近两周' },
  { value: 'last-30-days', label: '近一个月' },
  { value: 'last-90-days', label: '近三个月' },
]

const AI_SKILLS = [
  { id: 'weekly', label: '标准周报', icon: FileText },
  { id: 'month', label: '月度总结', icon: CalendarDays },
  { id: 'workload', label: '工作量评估', icon: BarChart3 },
  { id: 'review', label: '改动复盘', icon: Search },
  { id: 'insight', label: '真实功能总结', icon: GitBranch },
  { id: 'daily', label: '精简日报', icon: PencilLine },
]

const DEFAULT_AI_SYSTEM_PROMPT = '你是一个通用 AI 助手，也可以辅助 Git 工作复盘。优先回答用户当前问题；如果提供了 Git 统计上下文和真实改动依据，请优先基于文件级改动信息判断，不要只相信 commit 文案，也不要编造未提供的源代码或提交细节。使用中文 Markdown 输出。'
const PROJECT_AI_SUMMARY_SYSTEM_PROMPT = '你是代码改动理解助手。你会根据真实 diff 摘要、文件级变更和提交统计，判断每个项目实际完成了什么功能或调整。不要复述文件路径，不要照抄 commit 文案，不要编造没有依据的业务细节。'
const COMMIT_IMPACT_SYSTEM_PROMPT = '你是代码变更风险评审助手。你会根据单个 commit 的真实 diff 和文件变更，分析影响范围与风险点。必须基于证据，不要编造未提供的业务背景。'

const AI_SKILL_CONTEXT_OPTIONS: Array<{ value: AiSkillContextPolicy; label: string }> = [
  { value: 'none', label: '不使用 Git 上下文' },
  { value: 'optional-git', label: '可选 Git 上下文' },
  { value: 'required-git', label: '依赖 Git 上下文' },
]

const DEFAULT_AI_SKILLS: AiSkillConfig[] = [
  {
    id: 'weekly',
    label: '标准周报',
    description: '把 Git 记录整理成可直接发送的周报。',
    systemPrompt: '你是专业的研发周报助手，擅长把 Git 提交、合并记录、文件级真实改动和用户补充内容整理成清晰、可信的中文工作周报。',
    taskPrompt: '请生成一份标准研发周报。若 commit 文案质量较低，请以真实改动依据为主进行总结。',
    outputFormat: '输出结构：本周概览、项目进展、关键改动、风险与阻塞、下周计划。',
    contextPolicy: 'required-git',
    temperature: 0.3,
  },
  {
    id: 'month',
    label: '月度总结',
    description: '总结一个月的研发进展、产出和风险。',
    systemPrompt: '你是研发月报助手，擅长结合 Git 统计、合并记录和文件级真实改动，把阶段性工作整理成管理者可快速阅读的月度总结。',
    taskPrompt: '请生成一份月度工作总结。若 commit 文案质量较低，请以真实改动依据为主进行总结。',
    outputFormat: '输出结构：月度概览、主要成果、项目进展、风险问题、下月重点。',
    contextPolicy: 'required-git',
    temperature: 0.3,
  },
  {
    id: 'workload',
    label: '工作量评估',
    description: '按新需求拆解功能、复杂度、优先级、风险和验收注意点。',
    systemPrompt: '你是研发需求评估助手，擅长基于 Git 统计、文件级真实改动、diff 摘要和用户描述，把代码改动反推成新需求功能块，并评估复杂度、优先级、风险和验收注意点。',
    taskPrompt: '请按“新需求评估”的视角分析当前工作量。若 commit 文案质量较低，请以真实改动依据为主，把相近提交合并成可评审的功能项。',
    outputFormat: '输出结构：总体结论、功能排期评估表、高中低风险、验收注意点。功能排期评估表列为：功能项、需求类型、复杂度、优先级、预估耗时、前置依赖、风险等级、注意点；每个功能点都必须填写预估耗时，可用 0.5天、1天、2-3天、3-5天 这类粗粒度表达，不要输出具体日期。',
    contextPolicy: 'optional-git',
    temperature: 0.25,
  },
  {
    id: 'review',
    label: '改动复盘',
    description: '复盘代码改动目的、影响范围和风险。',
    systemPrompt: '你是代码改动复盘助手，关注变更目的、影响范围、潜在风险和后续验证建议。判断依据以文件级真实改动为主，commit 文案仅作参考。',
    taskPrompt: '请对当前改动进行复盘。不要按 commit 文案硬套分类，请根据真实改动依据推断实际影响。',
    outputFormat: '输出结构：改动摘要、影响范围、潜在风险、验证建议、后续动作。',
    contextPolicy: 'required-git',
    temperature: 0.25,
  },
  {
    id: 'insight',
    label: '真实功能总结',
    description: '根据文件级真实改动推断实际实现的功能。',
    systemPrompt: '你是代码变更理解助手，擅长根据 Git 文件级改动信息推断实际完成的功能、影响范围和可信度。不要依赖低质量 commit 文案，不要用正则分类，也不要编造未提供的源代码细节。',
    taskPrompt: '请基于真实改动依据，总结当前筛选范围内每个项目实际做了什么功能或改动。',
    outputFormat: '输出结构：整体结论、按项目功能总结、按提交关键改动、低可信或需补充上下文的部分、建议验证点。',
    contextPolicy: 'required-git',
    temperature: 0.2,
  },
  {
    id: 'daily',
    label: '精简日报',
    description: '生成短小、直接的日报内容。',
    systemPrompt: '你是日报写作助手，擅长结合文件级真实改动，把工作内容压缩成简洁、自然、可直接发送的中文日报。',
    taskPrompt: '请生成一份精简日报。若 commit 文案质量较低，请以真实改动依据为主进行总结。',
    outputFormat: '输出结构：今日完成、进行中、问题风险、明日计划。每部分尽量精简。',
    contextPolicy: 'optional-git',
    temperature: 0.35,
  },
]

type CalendarDay = {
  key: string
  value: string
  label: number
  inMonth: boolean
  isToday: boolean
  isStart: boolean
  isEnd: boolean
  inRange: boolean
}

type AiMessage = {
  id: string
  role: 'assistant' | 'user'
  content: string
  errorMessage?: string
  status: 'thinking' | 'streaming' | 'done' | 'error'
  streamingText?: string
}

type AiAssistantRequest = {
  systemPrompt: string
  prompt: string
  fallbackContent: string
  temperature: number
}

type ProjectAiSummaryInput = {
  id: string
  repoName: string
  repoPath: string
  totalCommits: number
  totalAdditions: number
  totalDeletions: number
  mergedBranchCount: number
  commits: CommitRecord[]
  mergedBranches: MergedBranchRecord[]
}

type CommitDiffRequest = {
  repoPath: string
  hash: string
}

type CommitDiffSummary = CommitDiffRequest & {
  excerpt: string
  files: CommitDiffFile[]
}

type CommitDiffFile = {
  path: string
  lines: CommitDiffLine[]
}

type CommitDiffLine = {
  kind: 'add' | 'delete' | 'hunk'
  content: string
}

type CommitImpactAnalysis = {
  scope: string
  high: string[]
  medium: string[]
  low: string[]
  verification: string[]
}

const config = ref<AppConfig>(createEmptyConfig())
const report = ref<WeeklyReport | null>(null)
const loading = ref(false)
const activeView = ref<'home' | 'settings'>('home')
const emailDraft = ref('')
const toast = ref('')
const errorText = ref('')
const scanToken = ref(0)
const selectedRepoPath = ref(ALL_PROJECTS)
const selectedProjectDetailPath = ref('')
const savedTargetBranch = ref(DEFAULT_TARGET_BRANCH)
const datePickerOpen = ref(false)
const datePickerRef = ref<HTMLElement | null>(null)
const pendingDateRangeStart = ref(config.value.dateRangeStart)
const pendingDateRangeEnd = ref(config.value.dateRangeEnd)
const calendarCursor = ref(startOfMonth(parseDateInput(config.value.dateRangeStart) ?? new Date()))
const isAiAssistantOpen = ref(false)
const selectedAiModelId = ref(config.value.defaultAiModelId)
const aiMessages = ref<AiMessage[]>([])
const aiQuestion = ref('')
const selectedAiSkillIds = ref<string[]>([])
const aiSending = ref(false)
const aiConversationRef = ref<HTMLElement | null>(null)
const projectAiSummaries = ref<Record<string, string>>({})
const projectAiSummarizing = ref(false)
const projectAiSummaryError = ref('')
const expandedCommitDiffKeys = ref<Record<string, boolean>>({})
const commitDiffLoading = ref<Record<string, boolean>>({})
const commitDiffErrors = ref<Record<string, string>>({})
const commitDiffSummaries = ref<Record<string, CommitDiffSummary>>({})
const commitImpactLoading = ref<Record<string, boolean>>({})
const commitImpactErrors = ref<Record<string, string>>({})
const commitImpactAnalyses = ref<Record<string, CommitImpactAnalysis>>({})
const aiModelEditorOpen = ref(false)
const editingAiModelId = ref<string | null>(null)
const aiModelDraft = ref<AiModelConfig>(createBlankAiModel())
const editingAiSkillId = ref(config.value.aiSkills[0]?.id ?? 'weekly')
let aiStreamRequestId = 0
let aiStreamTimer: ReturnType<typeof window.setTimeout> | null = null
let projectAiSummaryRunId = 0

const hasScanDirs = computed(() => config.value.scanDirs.length > 0)
const hasEmails = computed(() => config.value.emails.length > 0)
const canCopy = computed(() => Boolean(report.value && (filteredCommits.value.length > 0 || filteredMergedBranches.value.length > 0)))
const issueText = computed(() => {
  if (!report.value?.issues.length) return ''
  return report.value.issues[0].message
})
const emptyText = computed(() => {
  if (!hasScanDirs.value) return '请添加本地代码目录以开始扫描'
  if (!hasEmails.value) return '请配置 Git 个人邮箱'
  if (issueText.value && !visibleRowCount.value) return issueText.value
  if (!loading.value && report.value && !visibleRowCount.value) return '所选区间暂无代码提交或合并记录'
  return ''
})

const summaryItems = computed(() => [
  { label: '提交数', value: filteredSummary.value.totalCommits, tone: 'text-textMain' },
  { label: '新增', value: `+${filteredSummary.value.totalAdditions}`, tone: 'text-gain' },
  { label: '删除', value: `-${filteredSummary.value.totalDeletions}`, tone: 'text-loss' },
  { label: '仓库', value: filteredSummary.value.repoCount, tone: 'text-primary' },
  { label: `已合${displayTargetBranch.value}`, value: filteredSummary.value.mergedBranchCount, tone: 'text-primary' },
])

const projectOptions = computed(() => {
  const options = new Map<string, string>()
  for (const commit of report.value?.commits ?? []) {
    options.set(commit.repoPath, commit.repoName)
  }
  for (const merge of report.value?.mergedBranches ?? []) {
    options.set(merge.repoPath, merge.repoName)
  }

  return Array.from(options, ([path, name]) => ({ path, name })).sort((left, right) => left.name.localeCompare(right.name))
})

const currentProjectName = computed(() => projectOptions.value.find((project) => project.path === selectedRepoPath.value)?.name ?? '')
const displayTargetBranch = computed(() => report.value?.targetBranch || normalizeTargetBranch(config.value.targetBranch))

const filteredCommits = computed(() => {
  const commits = report.value?.commits ?? []
  if (!selectedRepoPath.value) return commits
  return commits.filter((commit) => commit.repoPath === selectedRepoPath.value)
})

const filteredMergedBranches = computed(() => {
  const merges = report.value?.mergedBranches ?? []
  if (!selectedRepoPath.value) return merges
  return merges.filter((merge) => merge.repoPath === selectedRepoPath.value)
})

const filteredSummary = computed(() => {
  const repoPaths = new Set<string>()
  let totalAdditions = 0
  let totalDeletions = 0

  for (const commit of filteredCommits.value) {
    repoPaths.add(commit.repoPath)
    totalAdditions += commit.additions ?? 0
    totalDeletions += commit.deletions ?? 0
  }

  for (const merge of filteredMergedBranches.value) {
    repoPaths.add(merge.repoPath)
  }

  return {
    totalCommits: filteredCommits.value.length,
    totalAdditions,
    totalDeletions,
    repoCount: repoPaths.size,
    mergedBranchCount: filteredMergedBranches.value.length,
  }
})

const activityRows = computed(() => buildActivityRows(filteredCommits.value, filteredMergedBranches.value))
const projectSummaryRows = computed(() => buildProjectSummaryRows(filteredCommits.value, filteredMergedBranches.value))
const projectDetailCommits = computed(() => {
  if (!selectedProjectDetailPath.value) return []
  return (report.value?.commits ?? []).filter((commit) => commit.repoPath === selectedProjectDetailPath.value)
})
const projectDetailMergedBranches = computed(() => {
  if (!selectedProjectDetailPath.value) return []
  return (report.value?.mergedBranches ?? []).filter((merge) => merge.repoPath === selectedProjectDetailPath.value)
})
const projectDetailRows = computed(() => buildActivityRows(projectDetailCommits.value, projectDetailMergedBranches.value))
const selectedProjectDetailName = computed(() => {
  const path = selectedProjectDetailPath.value
  if (!path) return ''
  return projectOptions.value.find((project) => project.path === path)?.name
    ?? projectDetailCommits.value[0]?.repoName
    ?? projectDetailMergedBranches.value[0]?.repoName
    ?? ''
})
const visibleRowCount = computed(() => (selectedProjectDetailPath.value ? projectDetailRows.value.length : projectSummaryRows.value.length))
const visibleRowUnit = computed(() => (selectedProjectDetailPath.value ? '条记录' : '个项目'))
const dateRangeDisplay = computed(() => `${formatDateDisplay(config.value.dateRangeStart)} - ${formatDateDisplay(config.value.dateRangeEnd)}`)
const pendingDateRangeDisplay = computed(() => `${formatDateDisplay(pendingDateRangeStart.value)} - ${formatDateDisplay(pendingDateRangeEnd.value || pendingDateRangeStart.value)}`)
const activeQuickRange = computed(() => findMatchingQuickRange(config.value.dateRangeStart, config.value.dateRangeEnd))
const calendarTitle = computed(() => `${calendarCursor.value.getFullYear()} 年 ${calendarCursor.value.getMonth() + 1} 月`)
const calendarDays = computed(() => buildCalendarDays(calendarCursor.value, pendingDateRangeStart.value, pendingDateRangeEnd.value))
const selectedAiModel = computed(() => {
  return config.value.aiModels.find((model) => model.id === selectedAiModelId.value)
    ?? config.value.aiModels.find((model) => model.id === config.value.defaultAiModelId)
    ?? config.value.aiModels[0]
    ?? null
})
const aiModelSummary = computed(() => `${config.value.aiModels.length}个模型`)
const aiSkillSummary = computed(() => `${config.value.aiSkills.length}个技能`)
const selectedAiSkills = computed(() => selectedAiSkillIds.value.map(getAiSkillConfig).filter((skill): skill is AiSkillConfig => Boolean(skill)))
const selectedAiSkillLabels = computed(() => selectedAiSkills.value.map((skill) => skill.label))
const canSendAiMessage = computed(() => Boolean(aiQuestion.value.trim() || selectedAiSkillIds.value.length))

watch([activeView, isAiAssistantOpen, datePickerOpen], ([view, aiOpen, pickerOpen]) => {
  setAutoHideSuspended(view === 'settings' || aiOpen || pickerOpen)
})

watch(projectOptions, (options) => {
  if (selectedRepoPath.value && !options.some((project) => project.path === selectedRepoPath.value)) {
    selectedRepoPath.value = ALL_PROJECTS
  }
  if (selectedProjectDetailPath.value && !options.some((project) => project.path === selectedProjectDetailPath.value)) {
    selectedProjectDetailPath.value = ''
  }
})

watch(
  () => [config.value.defaultAiModelId, config.value.aiModels.map((model) => model.id).join('|')],
  () => {
    ensureSelectedAiModel()
  },
)

onMounted(async () => {
  document.addEventListener('click', handleDocumentClick)
  await loadConfig()
  await syncAutostartState()
  await hydrateEmailCandidates()
  if (hasScanDirs.value && hasEmails.value) {
    await refreshReport()
  }

  if (isTauriRuntime()) {
    listen('navigate-settings', () => {
      activeView.value = 'settings'
    })
    listen('refresh-requested', () => {
      refreshReport()
    })
  }
})

onBeforeUnmount(() => {
  document.removeEventListener('click', handleDocumentClick)
  stopAiStream()
})

async function loadConfig() {
  try {
    const loadedConfig = isTauriRuntime() ? await invoke<AppConfig>('load_config') : loadBrowserConfig()
    applyLoadedConfig(loadedConfig)
  } catch {
    applyLoadedConfig(createEmptyConfig())
  }
}

async function saveConfig() {
  if (!isTauriRuntime()) {
    saveBrowserConfig(config.value)
    return
  }
  await invoke('save_config', { config: config.value })
}

function applyLoadedConfig(loadedConfig: AppConfig) {
  const targetBranch = normalizeTargetBranch(loadedConfig.targetBranch)
  const nextConfig = { ...createEmptyConfig(), ...loadedConfig, targetBranch }
  const normalizedRange = normalizeDateRangeValues(nextConfig.dateRangeStart, nextConfig.dateRangeEnd)
  const aiModels = normalizeAiModels(nextConfig.aiModels)
  const aiSkills = normalizeAiSkills(nextConfig.aiSkills)
  const defaultAiModelId = normalizeDefaultAiModelId(nextConfig.defaultAiModelId, aiModels)
  config.value = {
    ...nextConfig,
    dateRangeStart: normalizedRange.start,
    dateRangeEnd: normalizedRange.end,
    aiModels,
    aiSkills,
    defaultAiModelId,
  }
  savedTargetBranch.value = targetBranch
  selectedAiModelId.value = defaultAiModelId
  if (!aiSkills.some((skill) => skill.id === editingAiSkillId.value)) {
    editingAiSkillId.value = aiSkills[0]?.id ?? ''
  }
  calendarCursor.value = startOfMonth(parseDateInput(normalizedRange.start) ?? new Date())
}

function isTauriRuntime() {
  return isTauri()
}

function loadBrowserConfig() {
  try {
    const rawConfig = window.localStorage.getItem(BROWSER_CONFIG_STORAGE_KEY)
    if (!rawConfig) return createEmptyConfig()
    return { ...createEmptyConfig(), ...JSON.parse(rawConfig) } as AppConfig
  } catch {
    return createEmptyConfig()
  }
}

function saveBrowserConfig(nextConfig: AppConfig) {
  window.localStorage.setItem(BROWSER_CONFIG_STORAGE_KEY, JSON.stringify(nextConfig))
}

function resetBrowserConfig() {
  window.localStorage.removeItem(BROWSER_CONFIG_STORAGE_KEY)
}

async function resetConfig() {
  config.value = createEmptyConfig()
  report.value = null
  selectedRepoPath.value = ALL_PROJECTS
  selectedProjectDetailPath.value = ''
  savedTargetBranch.value = DEFAULT_TARGET_BRANCH
  selectedAiModelId.value = config.value.defaultAiModelId
  emailDraft.value = ''
  aiMessages.value = []
  aiQuestion.value = ''
  selectedAiSkillIds.value = []
  aiSending.value = false
  projectAiSummaries.value = {}
  projectAiSummarizing.value = false
  projectAiSummaryError.value = ''
  resetCommitDiffState()
  projectAiSummaryRunId += 1
  isAiAssistantOpen.value = false
  datePickerOpen.value = false
  aiModelEditorOpen.value = false
  editingAiModelId.value = null
  editingAiSkillId.value = config.value.aiSkills[0]?.id ?? ''
  errorText.value = ''
  try {
    if (isTauriRuntime()) {
      await invoke('reset_config')
    } else {
      resetBrowserConfig()
    }
    await syncAutostartState()
    showToast('已重置')
  } catch (error) {
    errorText.value = readableError(error)
  }
}

async function setAutoHideSuspended(suspended: boolean) {
  if (!isTauriRuntime()) return
  try {
    await invoke('set_auto_hide_suspended', { suspended })
  } catch {
    // Browser preview has no Tauri backend.
  }
}

async function addDirectory() {
  try {
    await setAutoHideSuspended(true)
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择代码目录',
    })
    const path = Array.isArray(selected) ? selected[0] : selected
    if (!path) return
    if (!config.value.scanDirs.includes(path)) {
      config.value.scanDirs = [...config.value.scanDirs, path]
      await saveConfig()
      await hydrateEmailCandidates()
      if (hasEmails.value) await refreshReport()
    }
  } catch (error) {
    errorText.value = readableError(error)
  } finally {
    await setAutoHideSuspended(activeView.value === 'settings')
  }
}

async function removeDirectory(path: string) {
  config.value.scanDirs = config.value.scanDirs.filter((item) => item !== path)
  await saveConfig()
  if (hasScanDirs.value && hasEmails.value) {
    await refreshReport()
  } else {
    report.value = null
  }
}

async function hydrateEmailCandidates() {
  if (!isTauriRuntime()) return
  try {
    const candidates = await invoke<string[]>('get_email_candidates', {
      scanDirs: config.value.scanDirs,
    })
    const merged = new Set([...config.value.emails, ...candidates].map((email) => normalizeEmail(email)).filter(Boolean))
    const nextEmails = Array.from(merged)
    if (nextEmails.length !== config.value.emails.length) {
      config.value.emails = nextEmails
      await saveConfig()
    }
  } catch {
    // Email auto-detection is a convenience, not a blocker.
  }
}

async function addEmail() {
  const email = normalizeEmail(emailDraft.value)
  if (!email || !email.includes('@')) return
  if (!config.value.emails.includes(email)) {
    config.value.emails = [...config.value.emails, email]
    await saveConfig()
    if (hasScanDirs.value) await refreshReport()
  }
  emailDraft.value = ''
  await nextTick()
}

async function removeEmail(email: string) {
  config.value.emails = config.value.emails.filter((item) => item !== email)
  await saveConfig()
  if (hasEmails.value && hasScanDirs.value) {
    await refreshReport()
  } else {
    report.value = null
  }
}

async function toggleAutostart() {
  const nextValue = !config.value.autostart
  if (!isTauriRuntime()) {
    config.value.autostart = nextValue
    await saveConfig()
    showToast(config.value.autostart ? '已开启' : '已关闭')
    return
  }

  try {
    if (nextValue) {
      await enable()
    } else {
      await disable()
    }
    config.value.autostart = await isEnabled()
    await saveConfig()
    showToast(config.value.autostart ? '已开启' : '已关闭')
  } catch (error) {
    errorText.value = readableError(error)
  }
}

async function saveTargetBranch() {
  const nextBranch = normalizeTargetBranch(config.value.targetBranch)
  config.value.targetBranch = nextBranch
  if (nextBranch === savedTargetBranch.value) return

  try {
    savedTargetBranch.value = nextBranch
    await saveConfig()
    if (hasScanDirs.value && hasEmails.value) {
      await refreshReport()
    }
  } catch (error) {
    errorText.value = readableError(error)
  }
}

async function saveAllSettings() {
  const previousTargetBranch = savedTargetBranch.value
  config.value.targetBranch = normalizeTargetBranch(config.value.targetBranch)
  config.value.aiModels = normalizeAiModels(config.value.aiModels)
  config.value.aiSkills = normalizeAiSkills(config.value.aiSkills)
  config.value.defaultAiModelId = normalizeDefaultAiModelId(config.value.defaultAiModelId, config.value.aiModels)

  try {
    await saveConfig()
    savedTargetBranch.value = config.value.targetBranch
    selectedAiModelId.value = config.value.defaultAiModelId
    showToast('已保存')
    if (config.value.targetBranch !== previousTargetBranch && hasScanDirs.value && hasEmails.value) {
      await refreshReport()
    }
  } catch (error) {
    errorText.value = readableError(error)
  }
}

async function saveDateRange() {
  const normalizedRange = normalizeDateRangeValues(config.value.dateRangeStart, config.value.dateRangeEnd)
  if (normalizedRange.error) {
    errorText.value = normalizedRange.error
    return
  }

  config.value.dateRangeStart = normalizedRange.start
  config.value.dateRangeEnd = normalizedRange.end
  selectedProjectDetailPath.value = ''
  errorText.value = ''

  try {
    await saveConfig()
    if (hasScanDirs.value && hasEmails.value) {
      await refreshReport()
    }
  } catch (error) {
    errorText.value = readableError(error)
  }
}

async function resetDateRangeToThisWeek() {
  await applyQuickRange('this-week')
}

async function resetDateRangeToThisMonth() {
  await applyQuickRange('this-month')
}

async function syncAutostartState() {
  if (!isTauriRuntime()) return
  try {
    config.value.autostart = await isEnabled()
    await saveConfig()
  } catch {
    config.value.autostart = false
  }
}

async function refreshReport() {
  if (!isTauriRuntime()) {
    showToast('浏览器预览不可扫描')
    return
  }
  if (!hasScanDirs.value || !hasEmails.value || loading.value) return
  const currentToken = scanToken.value + 1
  scanToken.value = currentToken
  loading.value = true
  errorText.value = ''
  projectAiSummaryError.value = ''
  resetCommitDiffState()
  projectAiSummaryRunId += 1
  config.value.targetBranch = normalizeTargetBranch(config.value.targetBranch)
  const normalizedRange = normalizeDateRangeValues(config.value.dateRangeStart, config.value.dateRangeEnd)
  if (normalizedRange.error) {
    errorText.value = normalizedRange.error
    loading.value = false
    return
  }
  config.value.dateRangeStart = normalizedRange.start
  config.value.dateRangeEnd = normalizedRange.end
  try {
    selectedProjectDetailPath.value = ''
    const nextReport = await invoke<WeeklyReport>('scan_weekly_report', {
      config: config.value,
    })
    if (scanToken.value === currentToken) {
      report.value = nextReport
      projectAiSummaries.value = {}
      savedTargetBranch.value = nextReport.targetBranch
      void nextTick().then(() => {
        if (scanToken.value === currentToken) {
          generateProjectAiSummaries({ auto: true })
        }
      })
    }
  } catch (error) {
    errorText.value = readableError(error)
  } finally {
    if (scanToken.value === currentToken) {
      loading.value = false
    }
  }
}

async function copyWeeklyReport() {
  if (!report.value) return
  try {
    const commits = selectedProjectDetailPath.value ? projectDetailCommits.value : filteredCommits.value
    const mergedBranches = selectedProjectDetailPath.value ? projectDetailMergedBranches.value : filteredMergedBranches.value
    const projectName = selectedProjectDetailPath.value ? selectedProjectDetailName.value : currentProjectName.value
    await copyText(buildMarkdown(report.value, commits, mergedBranches, projectName))
    showToast('已复制')
  } catch (error) {
    errorText.value = readableError(error)
  }
}

async function copyProjectSummary(summary: string) {
  const value = summary.trim()
  if (!value) return

  try {
    await copyText(value)
    showToast('已复制摘要')
  } catch (error) {
    errorText.value = readableError(error)
  }
}

async function generateProjectAiSummaries(options: { auto?: boolean } = {}) {
  if (projectAiSummarizing.value || !projectSummaryRows.value.length) return

  const model = getDeepSeekModel()
  const unavailableReason = getDeepSeekModelUnavailableReason(model)
  if (unavailableReason || !model) {
    projectAiSummaryError.value = unavailableReason
    if (!options.auto) {
      errorText.value = unavailableReason
      showToast('请先配置DeepSeek')
    }
    return
  }

  const inputs = buildProjectAiSummaryInputs(filteredCommits.value, filteredMergedBranches.value)
    .slice(0, MAX_AI_PROJECT_SUMMARY_PROJECTS)
  if (!inputs.length) return

  const runId = projectAiSummaryRunId + 1
  projectAiSummaryRunId = runId
  projectAiSummarizing.value = true
  projectAiSummaryError.value = ''
  errorText.value = ''

  try {
    const diffSummaryMap = await loadCommitDiffSummaryMap(inputs)
    if (runId !== projectAiSummaryRunId) return

    const content = await requestModelMessage(model, {
      systemPrompt: PROJECT_AI_SUMMARY_SYSTEM_PROMPT,
      prompt: buildProjectAiSummaryPrompt(inputs, diffSummaryMap),
      fallbackContent: '',
      temperature: 0.2,
    })
    if (runId !== projectAiSummaryRunId) return

    const summaries = parseProjectAiSummaryResponse(content, inputs)
    if (!summaries.size) {
      throw new Error('模型没有返回可用的项目摘要')
    }

    const nextSummaries = { ...projectAiSummaries.value }
    for (const input of inputs) {
      const summary = summaries.get(input.id)
      if (summary) {
        nextSummaries[buildProjectAiSummaryKey(input.repoPath)] = summary
      }
    }
    projectAiSummaries.value = nextSummaries
    if (!options.auto) showToast('已生成项目摘要')
  } catch (error) {
    if (runId === projectAiSummaryRunId) {
      projectAiSummaryError.value = readableError(error)
      errorText.value = readableError(error)
    }
  } finally {
    if (runId === projectAiSummaryRunId) {
      projectAiSummarizing.value = false
    }
  }
}

async function copyText(value: string) {
  try {
    await writeText(value)
    return
  } catch {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value)
      return
    }
    throw new Error('复制失败')
  }
}

function handleDocumentClick(event: MouseEvent) {
  if (!datePickerOpen.value) return
  const target = event.target
  if (target instanceof Node && datePickerRef.value?.contains(target)) return
  datePickerOpen.value = false
}

function toggleDatePicker() {
  if (datePickerOpen.value) {
    datePickerOpen.value = false
    return
  }
  pendingDateRangeStart.value = config.value.dateRangeStart
  pendingDateRangeEnd.value = config.value.dateRangeEnd
  calendarCursor.value = startOfMonth(parseDateInput(config.value.dateRangeStart) ?? new Date())
  datePickerOpen.value = true
}

function shiftCalendarMonth(offset: number) {
  calendarCursor.value = new Date(calendarCursor.value.getFullYear(), calendarCursor.value.getMonth() + offset, 1)
}

function selectCalendarDate(day: CalendarDay) {
  if (!pendingDateRangeStart.value || (pendingDateRangeStart.value && pendingDateRangeEnd.value)) {
    pendingDateRangeStart.value = day.value
    pendingDateRangeEnd.value = ''
    return
  }

  if (day.value < pendingDateRangeStart.value) {
    pendingDateRangeStart.value = day.value
    pendingDateRangeEnd.value = ''
    return
  }

  pendingDateRangeEnd.value = day.value
}

async function applyPendingDateRange() {
  if (!pendingDateRangeStart.value) return
  config.value.dateRangeStart = pendingDateRangeStart.value
  config.value.dateRangeEnd = pendingDateRangeEnd.value || pendingDateRangeStart.value
  await saveDateRange()
  if (!errorText.value) datePickerOpen.value = false
}

async function applyQuickRange(value: string) {
  if (value === CUSTOM_QUICK_RANGE) return
  const range = getQuickDateRange(value)
  if (!range) return

  config.value.dateRangeStart = range.start
  config.value.dateRangeEnd = range.end
  pendingDateRangeStart.value = range.start
  pendingDateRangeEnd.value = range.end
  calendarCursor.value = startOfMonth(parseDateInput(range.start) ?? new Date())
  datePickerOpen.value = false
  await saveDateRange()
}

function handleQuickRangeChange(event: Event) {
  const target = event.target as HTMLSelectElement
  applyQuickRange(target.value)
}

function openProjectDetail(repoPath: string) {
  selectedProjectDetailPath.value = repoPath
}

function closeProjectDetail() {
  selectedProjectDetailPath.value = ''
}

async function toggleCommitDiff(row: ReturnType<typeof buildActivityRows>[number]) {
  if (row.kind !== 'commit') return

  const key = row.key
  const nextOpen = !expandedCommitDiffKeys.value[key]
  expandedCommitDiffKeys.value = {
    ...expandedCommitDiffKeys.value,
    [key]: nextOpen,
  }

  if (nextOpen && !commitDiffSummaries.value[key] && !commitDiffLoading.value[key]) {
    await loadCommitDiff(row.repoPath, row.hash, key)
  }
}

async function loadCommitDiff(repoPath: string, hash: string, key: string) {
  commitDiffLoading.value = {
    ...commitDiffLoading.value,
    [key]: true,
  }
  commitDiffErrors.value = {
    ...commitDiffErrors.value,
    [key]: '',
  }

  try {
    const summaries = await invoke<CommitDiffSummary[]>('get_commit_diff_summaries', {
      requests: [{ repoPath, hash }],
    })
    const summary = summaries[0]
    if (!summary) {
      throw new Error('未读取到该提交的 diff')
    }
    commitDiffSummaries.value = {
      ...commitDiffSummaries.value,
      [key]: summary,
    }
  } catch (error) {
    commitDiffErrors.value = {
      ...commitDiffErrors.value,
      [key]: readableError(error),
    }
  } finally {
    commitDiffLoading.value = {
      ...commitDiffLoading.value,
      [key]: false,
    }
  }
}

async function analyzeCommitImpact(row: ReturnType<typeof buildActivityRows>[number]) {
  if (row.kind !== 'commit' || commitImpactLoading.value[row.key]) return

  const model = getDeepSeekModel()
  const unavailableReason = getDeepSeekModelUnavailableReason(model)
  if (unavailableReason || !model) {
    commitImpactErrors.value = {
      ...commitImpactErrors.value,
      [row.key]: unavailableReason,
    }
    showToast('请先配置DeepSeek')
    return
  }

  if (!commitDiffSummaries.value[row.key]) {
    await loadCommitDiff(row.repoPath, row.hash, row.key)
  }

  const diffSummary = commitDiffSummaries.value[row.key]
  if (!diffSummary) return

  commitImpactLoading.value = {
    ...commitImpactLoading.value,
    [row.key]: true,
  }
  commitImpactErrors.value = {
    ...commitImpactErrors.value,
    [row.key]: '',
  }

  try {
    const content = await requestModelMessage(model, {
      systemPrompt: COMMIT_IMPACT_SYSTEM_PROMPT,
      prompt: buildCommitImpactPrompt(row, diffSummary),
      fallbackContent: '',
      temperature: 0.15,
    })
    commitImpactAnalyses.value = {
      ...commitImpactAnalyses.value,
      [row.key]: parseCommitImpactResponse(content),
    }
  } catch (error) {
    commitImpactErrors.value = {
      ...commitImpactErrors.value,
      [row.key]: readableError(error),
    }
  } finally {
    commitImpactLoading.value = {
      ...commitImpactLoading.value,
      [row.key]: false,
    }
  }
}

function buildCommitImpactPrompt(row: ReturnType<typeof buildActivityRows>[number], diffSummary: CommitDiffSummary) {
  if (row.kind !== 'commit') return ''

  return [
    '请分析这个 commit 的影响范围与风险点。',
    '',
    '输出要求：',
    '- 只输出 JSON，不要 Markdown，不要代码块。',
    '- JSON 格式：{"scope":"一句话影响范围","high":["高风险1"],"medium":["中风险1"],"low":["低风险1"],"verification":["验证建议1"]}',
    '- high / medium / low / verification 每组最多 3 条。',
    '- 如果没有某级风险，返回空数组。',
    '- 风险判断必须来自 diff 内容和文件变更，不要只复述 commit 文案。',
    '',
    '## Commit 信息',
    `项目：${row.repoName}`,
    `提交：${row.subject}`,
    `时间：${formatDate(row.date)}`,
    `代码变更：+${row.additions ?? 0}/-${row.deletions ?? 0}`,
    `文件变更：${formatCommitFilesForPrompt(row.files, MAX_AI_FILES_PER_COMMIT)}`,
    '',
    '## 真实 diff',
    formatDiffSummaryForPrompt(diffSummary),
  ].join('\n')
}

function formatDiffSummaryForPrompt(diffSummary: CommitDiffSummary) {
  if (!diffSummary.files.length) return diffSummary.excerpt || '无可用文本 diff'

  return diffSummary.files
    .slice(0, 12)
    .map((file) => [
      `### ${file.path}`,
      ...file.lines.slice(0, 80).map((line) => `${getDiffLinePrefix(line.kind)} ${line.content}`),
    ].join('\n'))
    .join('\n\n')
}

function parseCommitImpactResponse(content: string): CommitImpactAnalysis {
  const data = parseModelJson(content)
  if (!data || typeof data !== 'object') {
    throw new Error('模型返回格式不是有效影响分析')
  }

  const payload = data as Record<string, unknown>
  return {
    scope: cleanOneSentenceSummary(String(payload.scope ?? '')),
    high: normalizeImpactList(payload.high),
    medium: normalizeImpactList(payload.medium),
    low: normalizeImpactList(payload.low),
    verification: normalizeImpactList(payload.verification),
  }
}

function normalizeImpactList(value: unknown) {
  if (!Array.isArray(value)) return []
  return value
    .map((item) => cleanOneSentenceSummary(String(item ?? '')))
    .filter(Boolean)
    .slice(0, 3)
}

function resetCommitDiffState() {
  expandedCommitDiffKeys.value = {}
  commitDiffLoading.value = {}
  commitDiffErrors.value = {}
  commitDiffSummaries.value = {}
  commitImpactLoading.value = {}
  commitImpactErrors.value = {}
  commitImpactAnalyses.value = {}
}

function getCommitDiffSummary(key: string) {
  return commitDiffSummaries.value[key]
}

function isCommitDiffOpen(key: string) {
  return Boolean(expandedCommitDiffKeys.value[key])
}

function isCommitDiffLoading(key: string) {
  return Boolean(commitDiffLoading.value[key])
}

function getCommitDiffError(key: string) {
  return commitDiffErrors.value[key] ?? ''
}

function isCommitImpactLoading(key: string) {
  return Boolean(commitImpactLoading.value[key])
}

function getCommitImpactError(key: string) {
  return commitImpactErrors.value[key] ?? ''
}

function getCommitImpactAnalysis(key: string) {
  return commitImpactAnalyses.value[key]
}

function getDiffLineClass(kind: CommitDiffLine['kind']) {
  if (kind === 'add') return 'bg-green-50 text-green-800'
  if (kind === 'delete') return 'bg-red-50 text-red-800'
  return 'bg-blue-50 text-textMuted'
}

function getDiffLinePrefix(kind: CommitDiffLine['kind']) {
  if (kind === 'add') return '+'
  if (kind === 'delete') return '-'
  return '@@'
}

function getCalendarDayClass(day: CalendarDay) {
  const base = 'grid h-8 w-8 place-items-center rounded-[6px] text-[12px] font-medium transition'
  if (day.isStart || day.isEnd) return `${base} bg-primary text-white`
  if (day.inRange) return `${base} bg-blue-50 text-primary`
  if (day.isToday) return `${base} border border-primary text-primary hover:bg-blue-50`
  if (!day.inMonth) return `${base} text-gray-300 hover:bg-panel`
  return `${base} text-textMain hover:bg-panel`
}

function toggleAiAssistant() {
  if (isAiAssistantOpen.value) {
    closeAiAssistant()
  } else {
    openAiAssistant()
  }
}

function openAiAssistant() {
  ensureSelectedAiModel()
  isAiAssistantOpen.value = true
  nextTick(scrollAiConversationToBottom)
}

function closeAiAssistant() {
  isAiAssistantOpen.value = false
}

function clearAiHistory() {
  stopAiStream()
  aiMessages.value = []
  aiQuestion.value = ''
  selectedAiSkillIds.value = []
  aiSending.value = false
  showToast('已清空')
}

function openAiSettings() {
  closeAiAssistant()
  activeView.value = 'settings'
}

function toggleAiSkill(skillId: string) {
  if (aiSending.value) return
  selectedAiSkillIds.value = selectedAiSkillIds.value.includes(skillId) ? [] : [skillId]
}

function isAiSkillSelected(skillId: string) {
  return selectedAiSkillIds.value.includes(skillId)
}

async function sendAiQuestion() {
  const question = aiQuestion.value.trim()
  const skillIds = [...selectedAiSkillIds.value]
  const skillLabels = selectedAiSkillLabels.value
  const selectedSkillId = skillIds[0] ?? ''
  if (aiSending.value || (!question && !skillIds.length)) return

  aiQuestion.value = ''
  selectedAiSkillIds.value = []
  aiSending.value = true
  aiMessages.value.push(createUserMessage(buildAiUserMessageContent(skillLabels, question)))
  const assistantMessage = createAssistantMessage()
  aiMessages.value.push(assistantMessage)
  await nextTick()
  scrollAiConversationToBottom()
  try {
    await startAiAssistantReply(assistantMessage.id, buildAiAssistantRequest(selectedSkillId, question))
  } finally {
    aiSending.value = false
  }
}

async function copyAssistantMessage(content: string) {
  try {
    await copyText(content)
    showToast('已复制')
  } catch (error) {
    errorText.value = readableError(error)
  }
}

function createUserMessage(content: string): AiMessage {
  return {
    id: createId('user'),
    role: 'user',
    content,
    status: 'done',
  }
}

function createAssistantMessage(): AiMessage {
  return {
    id: createId('assistant'),
    role: 'assistant',
    content: '',
    status: 'thinking',
    streamingText: '',
  }
}

function scrollAiConversationToBottom() {
  const container = aiConversationRef.value
  if (container) container.scrollTop = container.scrollHeight
}

function updateAiMessage(messageId: string, updater: (message: AiMessage) => AiMessage) {
  aiMessages.value = aiMessages.value.map((message) => (message.id === messageId ? updater(message) : message))
}

function stopAiStream() {
  aiStreamRequestId += 1
  if (aiStreamTimer) {
    window.clearTimeout(aiStreamTimer)
    aiStreamTimer = null
  }
}

function streamAssistantResponse(messageId: string, content: string) {
  stopAiStream()
  const currentRequestId = aiStreamRequestId
  const chunks = splitStreamingChunks(content)
  let currentIndex = 0
  let renderedContent = ''

  updateAiMessage(messageId, (message) => ({
    ...message,
    content: '',
    errorMessage: '',
    status: 'thinking',
    streamingText: '',
  }))

  return new Promise<void>((resolve) => {
    const tick = () => {
      if (currentRequestId !== aiStreamRequestId) {
        resolve()
        return
      }

      if (currentIndex === 0) {
        updateAiMessage(messageId, (message) => ({
          ...message,
          status: 'streaming',
        }))
      }

      renderedContent += chunks[currentIndex] ?? ''
      currentIndex += 1

      updateAiMessage(messageId, (message) => ({
        ...message,
        status: 'streaming',
        streamingText: renderedContent,
      }))
      nextTick(scrollAiConversationToBottom)

      if (currentIndex >= chunks.length) {
        updateAiMessage(messageId, (message) => ({
          ...message,
          content: renderedContent,
          status: renderedContent ? 'done' : 'error',
          streamingText: '',
          errorMessage: renderedContent ? '' : '当前回答为空',
        }))
        aiStreamTimer = null
        nextTick(scrollAiConversationToBottom)
        resolve()
        return
      }

      aiStreamTimer = window.setTimeout(tick, 18)
    }

    aiStreamTimer = window.setTimeout(tick, 220)
  })
}

function splitStreamingChunks(content: string) {
  const chars = Array.from(content)
  const chunks: string[] = []
  for (let index = 0; index < chars.length; index += 3) {
    chunks.push(chars.slice(index, index + 3).join(''))
  }
  return chunks.length ? chunks : ['']
}

function getAiMessageDisplayContent(message: AiMessage) {
  return message.streamingText || message.content || ''
}

function renderAiMessageMarkdown(message: AiMessage) {
  return renderMarkdownToHtml(getAiMessageDisplayContent(message))
}

function buildAiUserMessageContent(skillLabels: string[], question: string) {
  if (!skillLabels.length) return question
  return [
    `已选技能：${skillLabels.join('、')}`,
    question ? `补充要求：${question}` : '',
  ]
    .filter(Boolean)
    .join('\n')
}

async function startAiAssistantReply(messageId: string, request: AiAssistantRequest) {
  const model = selectedAiModel.value
  const unavailableReason = getAiModelUnavailableReason(model)
  if (unavailableReason) {
    await streamAssistantResponse(messageId, buildAiFallbackNotice(`当前未调用模型：${unavailableReason}`, request.fallbackContent))
    return
  }

  try {
    await streamModelResponse(messageId, model, request)
  } catch (error) {
    await streamAssistantResponse(messageId, buildAiFallbackNotice(`模型请求失败：${readableError(error)}`, request.fallbackContent))
  }
}

function getAiModelUnavailableReason(model: AiModelConfig | null) {
  if (!model) return '请先在设置中添加并选择 AI 模型'
  if (!model.baseUrl) return `请先为「${model.name}」填写 Base URL`
  if (!model.model) return `请先为「${model.name}」填写模型ID`
  if (!model.apiKey) return `请先为「${model.name}」填写 API Key 并保存配置`
  return ''
}

function getDeepSeekModel() {
  return config.value.aiModels.find((model) => model.id === DEEPSEEK_MODEL_ID)
    ?? config.value.aiModels.find((model) => {
      const text = `${model.provider} ${model.name} ${model.model}`.toLowerCase()
      return text.includes('deepseek')
    })
    ?? null
}

function getDeepSeekModelUnavailableReason(model: AiModelConfig | null) {
  if (!model) return '请先在设置中添加 DeepSeek 模型'
  if (!model.baseUrl) return '请先为 DeepSeek 填写 Base URL'
  if (!model.model) return '请先为 DeepSeek 填写模型ID'
  if (!model.apiKey) return '请先为 DeepSeek 填写 API Key 并保存配置'
  return ''
}

function buildAiFallbackNotice(reason: string, fallbackContent: string) {
  return [
    `> ${reason}`,
    `> 当前展示的是本地兜底结果，不是模型返回内容。`,
    '',
    fallbackContent,
  ].join('\n')
}

async function streamModelResponse(messageId: string, model: AiModelConfig, request: AiAssistantRequest) {
  stopAiStream()
  const currentRequestId = aiStreamRequestId
  let streamedContent = ''

  updateAiMessage(messageId, (message) => ({
    ...message,
    content: '',
    errorMessage: '',
    status: 'thinking',
    streamingText: '',
  }))

  const response = await fetch(createChatCompletionsUrl(model.baseUrl), {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${model.apiKey}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      model: model.model,
      messages: [
        {
          role: 'system',
          content: request.systemPrompt,
        },
        {
          role: 'user',
          content: request.prompt,
        },
      ],
      stream: true,
      temperature: request.temperature,
    }),
  })

  if (!response.ok) {
    throw new Error(await readModelError(response))
  }

  if (!response.body) {
    const data = await response.json()
    streamedContent = extractModelMessage(data)
    finalizeAiMessage(messageId, streamedContent)
    return
  }

  updateAiMessage(messageId, (message) => ({
    ...message,
    status: 'streaming',
  }))

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let pendingText = ''

  while (true) {
    if (currentRequestId !== aiStreamRequestId) return

    const { done, value } = await reader.read()
    if (done) break

    pendingText += decoder.decode(value, { stream: true })
    const lines = pendingText.split('\n')
    pendingText = lines.pop() ?? ''

    for (const rawLine of lines) {
      const delta = parseOpenAIStreamLine(rawLine)
      if (!delta) continue
      streamedContent += delta
      updateAiMessage(messageId, (message) => ({
        ...message,
        status: 'streaming',
        streamingText: streamedContent,
      }))
      await nextTick()
      scrollAiConversationToBottom()
    }
  }

  const rest = decoder.decode()
  if (rest) {
    const delta = parseOpenAIStreamLine(rest)
    if (delta) streamedContent += delta
  }

  finalizeAiMessage(messageId, streamedContent)
}

async function requestModelMessage(model: AiModelConfig, request: AiAssistantRequest) {
  const response = await fetch(createChatCompletionsUrl(model.baseUrl), {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${model.apiKey}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      model: model.model,
      messages: [
        {
          role: 'system',
          content: request.systemPrompt,
        },
        {
          role: 'user',
          content: request.prompt,
        },
      ],
      stream: false,
      temperature: request.temperature,
    }),
  })

  if (!response.ok) {
    throw new Error(await readModelError(response))
  }

  return extractModelMessage(await response.json()).trim()
}

function finalizeAiMessage(messageId: string, content: string) {
  updateAiMessage(messageId, (message) => ({
    ...message,
    content,
    status: content.trim() ? 'done' : 'error',
    streamingText: '',
    errorMessage: content.trim() ? '' : '当前回答为空',
  }))
  nextTick(scrollAiConversationToBottom)
}

function createChatCompletionsUrl(baseUrl: string) {
  const normalizedBaseUrl = (baseUrl || '').trim().replace(/\/+$/g, '')
  if (!normalizedBaseUrl) throw new Error('请先配置模型 Base URL')
  if (normalizedBaseUrl.endsWith('/chat/completions')) return normalizedBaseUrl
  return `${normalizedBaseUrl}/chat/completions`
}

function parseOpenAIStreamLine(rawLine: string) {
  const line = rawLine.trim()
  if (!line || line.startsWith(':')) return ''
  const payload = line.startsWith('data:') ? line.slice(5).trim() : line
  if (!payload || payload === '[DONE]') return ''

  try {
    const data = JSON.parse(payload)
    return data.choices?.[0]?.delta?.content ?? data.choices?.[0]?.message?.content ?? data.content ?? ''
  } catch {
    return ''
  }
}

async function readModelError(response: Response) {
  try {
    const data = await response.json()
    return data.error?.message || data.message || `模型请求失败：${response.status}`
  } catch {
    return `模型请求失败：${response.status}`
  }
}

function extractModelMessage(data: unknown) {
  if (!data || typeof data !== 'object') return ''
  const payload = data as Record<string, any>
  return payload.choices?.[0]?.message?.content ?? payload.content ?? payload.message ?? ''
}

function buildAiAssistantRequest(skillId: string, question: string): AiAssistantRequest {
  const skill = skillId ? getAiSkillConfig(skillId) : null
  if (!skill) {
    return {
      systemPrompt: DEFAULT_AI_SYSTEM_PROMPT,
      prompt: buildAiQuestionPrompt(question),
      fallbackContent: buildAiQuestionResponse(question),
      temperature: 0.3,
    }
  }

  return {
    systemPrompt: skill.systemPrompt || DEFAULT_AI_SYSTEM_PROMPT,
    prompt: buildAiSkillPrompt(skill, question),
    fallbackContent: buildAiSkillResponse(skill.id, question),
    temperature: normalizeAiSkillTemperature(skill.temperature, 0.3),
  }
}

function buildAiSkillPrompt(skill: AiSkillConfig, question: string) {
  const fixedInstruction = buildAiSkillFixedInstruction(skill.id)
  const lines = [
    '用户选择了快捷技能。请结合技能配置和用户输入完成回复；如果用户输入与技能任务冲突，以用户输入为准。',
    '',
    '## 技能任务',
    skill.taskPrompt,
  ]

  if (question) lines.push('', '## 用户输入', question)
  if (skill.outputFormat) lines.push('', '## 输出要求', skill.outputFormat)
  if (fixedInstruction) lines.push('', '## 固定规则', fixedInstruction)
  return buildAiPrompt(lines.join('\n'), skill.contextPolicy)
}

function buildAiSkillFixedInstruction(skillId: string) {
  if (skillId !== 'workload') return ''

  return [
    '本次按“新需求工作量评估”处理，而不是单纯复盘过去提交。',
    '请把相近 commit / 文件变更合并成需求功能项，输出一张功能排期评估表。',
    '如果上面的技能配置与本固定规则冲突，以本固定规则为准。',
    '表格必须包含：功能项、需求类型、复杂度、优先级、预估耗时、前置依赖、风险等级、注意点。',
    '预估耗时要给每个功能点一个大概完成周期，可用 0.5天、1天、2-3天、3-5天 这类粗粒度表达，每一行都必须填写。',
    '不要输出具体日期范围或具体排期日历。',
    '复杂度只能使用：低 / 中 / 高；优先级只能使用：P0 / P1 / P2；风险等级只能使用：低 / 中 / 高。',
    '表格单元格不要使用 <br>、HTML 换行或多行列表；多个注意点用顿号、逗号或分号合并，过长说明放到表格后的列表。',
    '注意点要写验收、联调、回归、兼容性或上线风险，不要写空泛建议。',
  ].join('\n')
}

function buildAiQuestionPrompt(question: string) {
  const contextPolicy = visibleRowCount.value ? 'optional-git' : 'none'
  return buildAiPrompt(`请作为通用 AI 助手回答用户问题：\n${question}`, contextPolicy)
}

function getAiSkillLabel(skillId: string) {
  return getAiSkillConfig(skillId)?.label ?? 'AI技能'
}

function buildAiPrompt(task: string, contextPolicy: AiSkillContextPolicy = 'optional-git') {
  const lines = [task]
  const gitContext = buildAiGitContext(contextPolicy)
  if (gitContext) lines.push('', gitContext)
  lines.push('', '请用中文 Markdown 输出，优先回答用户问题；不要编造未提供的 Git 数据。')
  return lines.join('\n')
}

function buildAiGitContext(contextPolicy: AiSkillContextPolicy) {
  if (contextPolicy === 'none') return ''

  if (!visibleRowCount.value) {
    if (contextPolicy === 'required-git') {
      return [
        '## Git 上下文',
        '当前没有可用的 Git 提交或合并记录。若任务依赖 Git 数据，请说明缺少上下文；若用户输入已经提供足够信息，请继续完成用户请求。',
      ].join('\n')
    }
    return ''
  }

  const recentCommits = [...filteredCommits.value]
    .sort((left, right) => right.authorDate.localeCompare(left.authorDate))
    .slice(0, MAX_AI_COMMIT_CONTEXT)

  return [
    '## Git 上下文',
    `- 统计周期：${dateRangeDisplay.value}`,
    `- 项目筛选：${currentProjectName.value || '全部项目'}`,
    `- 目标分支：${displayTargetBranch.value}`,
    `- 提交数：${filteredSummary.value.totalCommits}`,
    `- 新增代码行：${filteredSummary.value.totalAdditions}`,
    `- 删除代码行：${filteredSummary.value.totalDeletions}`,
    `- 涉及项目数：${filteredSummary.value.repoCount}`,
    `- 已合 ${displayTargetBranch.value} 分支数：${filteredSummary.value.mergedBranchCount}`,
    '',
    '## 项目汇总',
    ...buildProjectSummaryBullets(8),
    '',
    '## 最近记录',
    ...activityRows.value.slice(0, 12).map((row) => {
      if (row.kind === 'merge') return `- ${row.repoName}：合并 ${row.sourceBranch ?? '未知分支'} -> ${row.targetBranch}，${row.subject}`
      return `- ${row.repoName}：提交 ${row.subject}，+${row.additions ?? 0}/-${row.deletions ?? 0}，真实改动：${row.fileSummary}`
    }),
    '',
    '## 真实改动依据',
    '以下信息来自 git numstat 的文件级变更清单，不依赖 commit 文案；若标题和文件改动冲突，请以文件改动为主，并对低可信判断明确说明。',
    ...recentCommits.map(buildAiCommitChangeContext),
  ].join('\n')
}

function buildAiSkillResponse(skillId: string, question = '') {
  if (!visibleRowCount.value) {
    return buildAiUnavailableLocalResponse(question, getAiSkillLabel(skillId))
  }

  if (skillId === 'month') return buildAiMonthSummary()
  if (skillId === 'workload') return buildAiWorkloadSummary()
  if (skillId === 'review') return buildAiReviewSummary()
  if (skillId === 'insight') return buildAiInsightSummary()
  if (skillId === 'daily') return buildAiDailySummary()
  return buildAiWeeklySummary()
}

function buildAiSelectedSkillsResponse(skillIds: string[], question: string) {
  const responses = skillIds.map((skillId) => {
    if (skillIds.length === 1) return buildAiSkillResponse(skillId, question)
    return `## ${getAiSkillLabel(skillId)}\n\n${buildAiSkillResponse(skillId, question)}`
  })

  return [
    ...responses,
    question ? `\n补充要求：${question}` : '',
  ]
    .join('\n\n')
    .trim()
}

function buildAiWeeklySummary() {
  const lines = [
    '本周工作周报已生成：',
    '',
    '### 本周工作汇总',
    `**统计周期**：${dateRangeDisplay.value}`,
    `**总提交**：${filteredSummary.value.totalCommits}次 | **代码变动**：+${filteredSummary.value.totalAdditions}/-${filteredSummary.value.totalDeletions}行 | **涉及项目**：${filteredSummary.value.repoCount}个`,
    `**已合 ${displayTargetBranch.value}**：${filteredSummary.value.mergedBranchCount}个分支`,
    '',
    ...buildProjectSummaryBullets(4),
  ]
  return lines.join('\n').trim()
}

function buildAiMonthSummary() {
  return [
    '月度工作总结已生成：',
    '',
    `当前筛选区间为 ${dateRangeDisplay.value}，共覆盖 ${filteredSummary.value.repoCount} 个项目。`,
    `累计提交 ${filteredSummary.value.totalCommits} 次，代码变动 +${filteredSummary.value.totalAdditions}/-${filteredSummary.value.totalDeletions} 行，已合 ${displayTargetBranch.value} 分支 ${filteredSummary.value.mergedBranchCount} 个。`,
    '',
    ...buildProjectSummaryBullets(5),
  ].join('\n').trim()
}

function buildAiWorkloadSummary() {
  const commitCount = filteredSummary.value.totalCommits
  const changedLines = filteredSummary.value.totalAdditions + filteredSummary.value.totalDeletions
  const conclusion =
    commitCount >= 20 || changedLines >= 1200
      ? '当前更像一组偏高复杂度的新需求集合，需要先拆分功能项并明确联调、回归和上线风险。'
      : commitCount >= 8 || changedLines >= 400
        ? '当前更像一组中等复杂度的新需求，需要按模块拆解功能项并确认前置依赖。'
        : '当前变更量不大，但仍建议按新需求口径确认功能边界和验收点。'
  const rows = projectSummaryRows.value.slice(0, 6)

  return [
    '### 工作量评估',
    '',
    `提交 ${commitCount} 次，代码变动 ${changedLines} 行，涉及项目 ${filteredSummary.value.repoCount} 个。`,
    conclusion,
    '',
    '### 功能排期评估表',
    '',
    '| 功能项 | 需求类型 | 复杂度 | 优先级 | 预估耗时 | 前置依赖 | 风险等级 | 注意点 |',
    '| --- | --- | --- | --- | --- | --- | --- | --- |',
    ...(rows.length ? rows.map(buildWorkloadFallbackTableRow) : ['| 暂无明确功能项 | 新需求 | 低 | P2 | 0.5天 | 需补充需求背景 | 低 | 补充业务目标、验收口径和测试路径 |']),
    '',
    '### 验收注意点',
    '',
    '- 先确认功能边界和是否影响登录、权限、接口、数据结构等关键链路。',
    '- 若涉及多个项目，需要明确联调顺序和回归范围。',
    '- 上线前至少覆盖新增路径、异常路径和老数据兼容。',
  ].join('\n').trim()
}

function buildWorkloadFallbackTableRow(row: ReturnType<typeof buildProjectSummaryRows>[number]) {
  const changedLines = row.totalAdditions + row.totalDeletions
  const complexity = changedLines >= 500 || row.totalCommits >= 10 ? '高' : changedLines >= 120 || row.totalCommits >= 4 ? '中' : '低'
  const priority = row.aiSummary ? 'P1' : complexity === '高' ? 'P1' : 'P2'
  const risk = row.mergedBranchCount > 0 || complexity === '高' ? '中' : '低'
  const duration = complexity === '高' ? '2-3天' : complexity === '中' ? '1天' : '0.5天'
  const feature = row.aiSummary || row.changeSummary || row.repoName
  return `| ${escapeMarkdownCell(feature)} | 新需求 | ${complexity} | ${priority} | ${duration} | 需求验收口径、接口/联调资源 | ${risk} | 重点确认影响范围、回归路径和上线兼容性 |`
}

function buildAiReviewSummary() {
  const rows = activityRows.value.slice(0, 6)
  return [
    '代码改动复盘：',
    '',
    ...rows.map((row) => {
      if (row.kind === 'merge') return `- ${row.repoName}：合并 ${row.sourceBranch ?? '未知分支'} -> ${row.targetBranch}，${row.subject}`
      return `- ${row.repoName}：${row.fileSummary !== '-' ? row.fileSummary : row.subject}`
    }),
  ].join('\n').trim()
}

function buildAiInsightSummary() {
  const commits = [...filteredCommits.value]
    .sort((left, right) => right.authorDate.localeCompare(left.authorDate))
    .slice(0, 8)

  return [
    '真实功能总结：',
    '',
    ...buildProjectSummaryBullets(5),
    '',
    '### 近期提交依据',
    ...commits.map((commit) => `- ${commit.repoName}：${summarizeCommitFiles(commit, 6)}；commit 文案：${commit.subject || commit.hash.slice(0, 7)}`),
  ].join('\n').trim()
}

function buildAiDailySummary() {
  const rows = activityRows.value.slice(0, 5)
  return [
    '精简日报：',
    '',
    `今日/当前区间完成 ${filteredSummary.value.totalCommits} 次提交，涉及 ${filteredSummary.value.repoCount} 个项目。`,
    ...rows.map((row) => {
      if (row.kind === 'merge') return `- ${row.repoName}：合并 ${row.sourceBranch ?? '未知分支'} -> ${row.targetBranch}`
      return `- ${row.repoName}：${row.fileSummary !== '-' ? row.fileSummary : row.subject}`
    }),
  ].join('\n').trim()
}

function buildAiQuestionResponse(question: string) {
  if (!visibleRowCount.value) {
    return buildAiUnavailableLocalResponse(question)
  }

  const normalized = question.toLowerCase()
  if (normalized.includes('月')) return buildAiMonthSummary()
  if (normalized.includes('工作量') || normalized.includes('评估')) return buildAiWorkloadSummary()
  if (normalized.includes('真实') || normalized.includes('功能') || normalized.includes('实际')) return buildAiInsightSummary()
  if (normalized.includes('改动') || normalized.includes('复盘')) return buildAiReviewSummary()
  if (normalized.includes('日报')) return buildAiDailySummary()

  return [
    `基于当前筛选数据回答：${question}`,
    '',
    `统计周期：${dateRangeDisplay.value}`,
    `提交 ${filteredSummary.value.totalCommits} 次，代码变动 +${filteredSummary.value.totalAdditions}/-${filteredSummary.value.totalDeletions} 行，涉及 ${filteredSummary.value.repoCount} 个项目。`,
    '',
    ...buildProjectSummaryBullets(3),
  ].join('\n').trim()
}

function buildAiUnavailableLocalResponse(question: string, skillLabel = '') {
  return [
    question ? `已收到你的问题：${question}` : skillLabel ? `已选择技能：${skillLabel}` : '已收到你的请求。',
    '',
    '当前没有生成真正的模型回复。本地兜底不能替代通用 AI 对话；模型配置可用后，这个请求会直接发送给模型，即使当前没有 Git 数据也可以正常回答。',
  ].join('\n')
}

function buildProjectSummaryBullets(limit: number) {
  const rows = projectSummaryRows.value.slice(0, limit)
  if (!rows.length) return ['- 暂无项目汇总']
  return rows.map((row) => `- ${row.repoName}：提交${row.totalCommits}次，代码+${row.totalAdditions}/-${row.totalDeletions}行，真实改动：${row.aiSummary || row.changeSummary}`)
}

function buildProjectAiSummaryInputs(commits: CommitRecord[], mergedBranches: MergedBranchRecord[]) {
  const grouped = new Map<string, Omit<ProjectAiSummaryInput, 'id'>>()

  for (const commit of commits) {
    const row = getProjectAiSummaryInput(grouped, commit.repoPath, commit.repoName)
    row.totalCommits += 1
    row.totalAdditions += commit.additions ?? 0
    row.totalDeletions += commit.deletions ?? 0
    row.commits.push(commit)
  }

  for (const merge of mergedBranches) {
    const row = getProjectAiSummaryInput(grouped, merge.repoPath, merge.repoName)
    row.mergedBranchCount += 1
    row.mergedBranches.push(merge)
  }

  return Array.from(grouped.values())
    .map((row, index) => ({
      ...row,
      id: `p${index + 1}`,
      commits: [...row.commits].sort((left, right) => right.authorDate.localeCompare(left.authorDate)).slice(0, MAX_AI_PROJECT_COMMITS),
      mergedBranches: [...row.mergedBranches].sort((left, right) => right.mergedAt.localeCompare(left.mergedAt)).slice(0, MAX_AI_PROJECT_MERGES),
    }))
    .sort((left, right) => right.totalCommits - left.totalCommits || left.repoName.localeCompare(right.repoName))
}

function getProjectAiSummaryInput(
  grouped: Map<string, Omit<ProjectAiSummaryInput, 'id'>>,
  repoPath: string,
  repoName: string,
) {
  const existing = grouped.get(repoPath)
  if (existing) return existing

  const row = {
    repoName,
    repoPath,
    totalCommits: 0,
    totalAdditions: 0,
    totalDeletions: 0,
    mergedBranchCount: 0,
    commits: [],
    mergedBranches: [],
  }
  grouped.set(repoPath, row)
  return row
}

async function loadCommitDiffSummaryMap(inputs: ProjectAiSummaryInput[]) {
  const summaryMap = new Map<string, string>()
  if (!isTauriRuntime()) return summaryMap

  const requests: CommitDiffRequest[] = []
  const seen = new Set<string>()
  for (const input of inputs) {
    for (const commit of input.commits) {
      const key = buildCommitDiffSummaryKey(commit.repoPath, commit.hash)
      if (seen.has(key)) continue
      seen.add(key)
      requests.push({ repoPath: commit.repoPath, hash: commit.hash })
    }
  }

  if (!requests.length) return summaryMap

  const summaries = await invoke<CommitDiffSummary[]>('get_commit_diff_summaries', { requests })
  for (const summary of summaries) {
    if (!summary.excerpt) continue
    summaryMap.set(buildCommitDiffSummaryKey(summary.repoPath, summary.hash), summary.excerpt)
  }
  return summaryMap
}

function buildProjectAiSummaryPrompt(inputs: ProjectAiSummaryInput[], diffSummaryMap: Map<string, string>) {
  return [
    '请根据下面每个项目的真实改动依据，为每个项目生成一句中文功能摘要。',
    '',
    '要求：',
    '- 每个项目只输出一句话，尽量控制在 18-45 个中文字符。',
    '- 说清楚“实际做了什么功能/调整/修复”，不要输出文件路径列表。',
    '- commit 文案只能辅助理解，真实 diff 摘要和文件变更优先级更高。',
    '- 如果依据不足，直接说明“主要调整了某某模块，具体业务需补充确认”。',
    '- 只输出 JSON 数组，不要输出 Markdown、解释或代码块。',
    '- JSON 格式：[{"id":"p1","summary":"一句话摘要"}]',
    '',
    '## 项目真实改动依据',
    ...inputs.map((input) => buildProjectAiSummaryPromptBlock(input, diffSummaryMap)),
  ].join('\n')
}

function buildProjectAiSummaryPromptBlock(input: ProjectAiSummaryInput, diffSummaryMap: Map<string, string>) {
  return [
    `### ${input.id} ${input.repoName}`,
    `统计：提交 ${input.totalCommits} 次，代码 +${input.totalAdditions}/-${input.totalDeletions}，已合分支 ${input.mergedBranchCount} 个`,
    input.mergedBranches.length ? `合并：${input.mergedBranches.map((merge) => merge.sourceBranch ?? merge.subject).join('；')}` : '',
    '提交与真实改动：',
    ...input.commits.map((commit) => buildCommitAiSummaryPromptLine(commit, diffSummaryMap)),
  ]
    .filter(Boolean)
    .join('\n')
}

function buildCommitAiSummaryPromptLine(commit: CommitRecord, diffSummaryMap: Map<string, string>) {
  const diffExcerpt = diffSummaryMap.get(buildCommitDiffSummaryKey(commit.repoPath, commit.hash))
  return [
    `- ${formatDate(commit.authorDate)} ${commit.subject || commit.hash.slice(0, 7)} (+${commit.additions ?? 0}/-${commit.deletions ?? 0})`,
    `  文件变更：${formatCommitFilesForPrompt(getCommitFiles(commit), MAX_AI_FILES_PER_COMMIT)}`,
    diffExcerpt ? `  真实diff摘要：\n${indentLines(diffExcerpt, '    ')}` : '',
  ]
    .filter(Boolean)
    .join('\n')
}

function parseProjectAiSummaryResponse(content: string, inputs: ProjectAiSummaryInput[]) {
  const inputIds = new Set(inputs.map((input) => input.id))
  const data = parseModelJson(content)
  const rows: unknown[] = Array.isArray(data)
    ? data
    : data && typeof data === 'object' && Array.isArray((data as Record<string, unknown>).projects)
      ? (data as Record<string, unknown>).projects as unknown[]
      : []
  const summaries = new Map<string, string>()

  for (const row of rows) {
    if (!row || typeof row !== 'object') continue
    const payload = row as Record<string, unknown>
    const id = String(payload.id ?? '').trim()
    const summary = cleanOneSentenceSummary(String(payload.summary ?? ''))
    if (!inputIds.has(id) || !summary) continue
    summaries.set(id, summary)
  }

  return summaries
}

function parseModelJson(content: string): unknown {
  const cleaned = content
    .replace(/```json/gi, '')
    .replace(/```/g, '')
    .trim()

  try {
    return JSON.parse(cleaned)
  } catch {
    const start = cleaned.indexOf('[')
    const end = cleaned.lastIndexOf(']')
    if (start >= 0 && end > start) {
      return JSON.parse(cleaned.slice(start, end + 1))
    }
    throw new Error('模型返回格式不是有效 JSON')
  }
}

function cleanOneSentenceSummary(value: string) {
  return truncateText(
    value
      .replace(/^[-*\d.、\s]+/g, '')
      .replace(/\s+/g, ' ')
      .trim(),
    80,
  )
}

function buildProjectAiSummaryKey(repoPath: string) {
  return [repoPath, config.value.dateRangeStart, config.value.dateRangeEnd, displayTargetBranch.value].join('\u{1f}')
}

function getProjectAiSummary(repoPath: string) {
  return projectAiSummaries.value[buildProjectAiSummaryKey(repoPath)]?.trim() ?? ''
}

function buildCommitDiffSummaryKey(repoPath: string, hash: string) {
  return `${repoPath}\u{1f}${hash}`
}

function indentLines(value: string, prefix: string) {
  return value
    .split('\n')
    .map((line) => `${prefix}${line}`)
    .join('\n')
}

function truncateText(value: string, maxLength: number) {
  const chars = Array.from(value)
  if (chars.length <= maxLength) return value
  return `${chars.slice(0, maxLength).join('')}…`
}

function buildAiCommitChangeContext(commit: CommitRecord) {
  const subject = commit.subject || commit.hash.slice(0, 7)
  const fileSummary = summarizeCommitFiles(commit, 6)
  const fileDetails = formatCommitFilesForPrompt(getCommitFiles(commit), MAX_AI_FILES_PER_COMMIT)

  return [
    `- ${commit.repoName} / ${formatDate(commit.authorDate)} / ${subject} / +${commit.additions ?? 0}/-${commit.deletions ?? 0}`,
    `  - 真实改动：${fileSummary}`,
    `  - 文件明细：${fileDetails}`,
  ].join('\n')
}

function formatCommitFilesForPrompt(files: CommitFileChange[], limit: number) {
  if (!files.length) return '无文件级变更清单'

  const sortedFiles = sortCommitFilesByChangeSize(files)
  const visibleFiles = sortedFiles.slice(0, limit).map(formatFileChangeForPrompt)
  const hiddenCount = Math.max(0, sortedFiles.length - visibleFiles.length)
  return hiddenCount ? `${visibleFiles.join('；')}；等 ${hiddenCount} 个文件` : visibleFiles.join('；')
}

function formatFileChangeForPrompt(file: CommitFileChange) {
  const additions = file.additions === null ? '?' : `+${file.additions}`
  const deletions = file.deletions === null ? '?' : `-${file.deletions}`
  return `${file.path} (${additions}/${deletions})`
}

function openAiModelEditor(model?: AiModelConfig) {
  editingAiModelId.value = model?.id ?? null
  aiModelDraft.value = model ? { ...model } : createBlankAiModel()
  aiModelEditorOpen.value = true
}

function closeAiModelEditor() {
  aiModelEditorOpen.value = false
  editingAiModelId.value = null
  aiModelDraft.value = createBlankAiModel()
}

async function saveAiModelDraft() {
  const draft = normalizeAiModel(aiModelDraft.value)
  if (!draft.name || !draft.model) {
    errorText.value = '请填写模型名称和模型ID'
    return
  }

  if (editingAiModelId.value) {
    config.value.aiModels = config.value.aiModels.map((model) => (model.id === editingAiModelId.value ? draft : model))
  } else {
    config.value.aiModels = [...config.value.aiModels, draft]
  }

  if (!config.value.defaultAiModelId || !config.value.aiModels.some((model) => model.id === config.value.defaultAiModelId)) {
    config.value.defaultAiModelId = draft.id
  }

  try {
    await saveConfig()
    ensureSelectedAiModel()
    closeAiModelEditor()
    showToast('已保存模型')
  } catch (error) {
    errorText.value = readableError(error)
  }
}

async function removeAiModel(id: string) {
  config.value.aiModels = config.value.aiModels.filter((model) => model.id !== id)
  config.value.defaultAiModelId = normalizeDefaultAiModelId(config.value.defaultAiModelId, config.value.aiModels)
  ensureSelectedAiModel()

  try {
    await saveConfig()
    showToast('已删除模型')
  } catch (error) {
    errorText.value = readableError(error)
  }
}

async function setDefaultAiModel(id: string) {
  config.value.defaultAiModelId = id
  selectedAiModelId.value = id
  try {
    await saveConfig()
    showToast('已设为默认')
  } catch (error) {
    errorText.value = readableError(error)
  }
}

function ensureSelectedAiModel() {
  if (config.value.aiModels.some((model) => model.id === selectedAiModelId.value)) return
  selectedAiModelId.value = normalizeDefaultAiModelId(config.value.defaultAiModelId, config.value.aiModels)
}

function openAiSkillEditor(skillId: string) {
  editingAiSkillId.value = skillId
}

async function saveAiSkillSettings() {
  config.value.aiSkills = normalizeAiSkills(config.value.aiSkills)
  try {
    await saveConfig()
    showToast('已保存技能')
  } catch (error) {
    errorText.value = readableError(error)
  }
}

async function resetAiSkill(skillId: string) {
  const defaultSkill = getDefaultAiSkillConfig(skillId)
  if (!defaultSkill) return
  config.value.aiSkills = config.value.aiSkills.map((skill) => (skill.id === skillId ? { ...defaultSkill } : skill))
  try {
    await saveConfig()
    showToast('已重置技能')
  } catch (error) {
    errorText.value = readableError(error)
  }
}

async function resetAllAiSkills() {
  config.value.aiSkills = createDefaultAiSkills()
  editingAiSkillId.value = config.value.aiSkills[0]?.id ?? ''
  try {
    await saveConfig()
    showToast('已重置技能')
  } catch (error) {
    errorText.value = readableError(error)
  }
}

function buildMarkdown(
  data: WeeklyReport,
  commits: CommitRecord[] = data.commits,
  mergedBranches: MergedBranchRecord[] = data.mergedBranches,
  projectName = '',
) {
  const grouped = new Map<string, CommitRecord[]>()
  for (const commit of commits) {
    const repoCommits = grouped.get(commit.repoName) ?? []
    repoCommits.push(commit)
    grouped.set(commit.repoName, repoCommits)
  }

  const groupedMerges = new Map<string, MergedBranchRecord[]>()
  for (const merge of mergedBranches) {
    const repoMerges = groupedMerges.get(merge.repoName) ?? []
    repoMerges.push(merge)
    groupedMerges.set(merge.repoName, repoMerges)
  }

  const totalAdditions = commits.reduce((sum, commit) => sum + (commit.additions ?? 0), 0)
  const totalDeletions = commits.reduce((sum, commit) => sum + (commit.deletions ?? 0), 0)
  const repoCount = new Set([...commits.map((commit) => commit.repoPath), ...mergedBranches.map((merge) => merge.repoPath)]).size
  const targetBranch = data.targetBranch || displayTargetBranch.value
  const projectSummaryRows = buildProjectSummaryRows(commits, mergedBranches)
  const lines = [
    '# GitSage 工作报告',
    '',
    `统计周期：${formatDate(data.weekStart)} - ${formatDate(data.weekEnd)}`,
    projectName ? `项目维度：${projectName}` : '',
    '',
    '## 总览',
    '',
    `- 提交次数：${commits.length}`,
    `- 新增代码行数：${totalAdditions}`,
    `- 删除代码行数：${totalDeletions}`,
    `- 涉及仓库数量：${repoCount}`,
    `- 已合 ${targetBranch} 分支数：${mergedBranches.length}`,
    '',
    '## 项目汇总表',
    '',
    '| 项目 | 提交 | 新增 | 删除 | 工作摘要 |',
    '| --- | ---: | ---: | ---: | --- |',
    ...projectSummaryRows.map(
      (row) =>
        `| ${escapeMarkdownCell(row.repoName)} | ${row.totalCommits} | +${row.totalAdditions} | -${row.totalDeletions} | ${escapeMarkdownCell(row.aiSummary || row.changeSummary)} |`,
    ),
    '',
    `## 已合 ${targetBranch} 分支`,
    '',
  ].filter((line, index, lines) => line || lines[index - 1] !== '')

  if (!projectSummaryRows.length) {
    lines.splice(lines.indexOf('| 项目 | 提交 | 新增 | 删除 | 工作摘要 |') + 2, 0, '| - | 0 | +0 | -0 | 所选区间暂无记录 |')
  }

  if (!mergedBranches.length) {
    lines.push(`所选区间暂无合入 ${targetBranch} 的分支记录`, '')
  } else {
    groupedMerges.forEach((merges, repoName) => {
      lines.push(`### ${repoName}`, '')
      merges.forEach((merge) => {
        lines.push(`- ${formatDate(merge.mergedAt)} ${merge.sourceBranch ?? '未知分支'} -> ${merge.targetBranch}：${merge.subject}`)
      })
      lines.push('')
    })
  }

  lines.push('## 提交明细', '')

  if (!commits.length) {
    lines.push('所选区间暂无代码提交记录')
  } else {
    grouped.forEach((repoCommits, repoName) => {
      lines.push(`### ${repoName}`, '')
      repoCommits.forEach((commit) => {
        const additions = commit.additions === null ? '-' : `+${commit.additions}`
        const deletions = commit.deletions === null ? '-' : `-${commit.deletions}`
        const fileSummary = summarizeCommitFiles(commit, 6)
        lines.push(`- ${formatDate(commit.authorDate)} ${commit.subject} (${additions} / ${deletions})`)
        if (fileSummary !== '-') {
          lines.push(`  - 真实改动：${fileSummary}`)
        }
      })
      lines.push('')
    })
  }

  if (data.issues.length) {
    lines.push('## 扫描提示', '')
    data.issues.forEach((issue) => {
      lines.push(`- ${issue.path ? `${issue.path}：` : ''}${issue.message}`)
    })
  }

  return lines.join('\n').trimEnd()
}

function buildActivityRows(commits: CommitRecord[], mergedBranches: MergedBranchRecord[]) {
  const commitRows = commits.map((commit) => ({
    kind: 'commit' as const,
    key: `commit-${commit.repoPath}-${commit.hash}`,
    repoName: commit.repoName,
    repoPath: commit.repoPath,
    hash: commit.hash,
    date: commit.authorDate,
    subject: commit.subject,
    additions: commit.additions,
    deletions: commit.deletions,
    files: getCommitFiles(commit),
    fileSummary: summarizeCommitFiles(commit),
  }))
  const mergeRows = mergedBranches.map((merge) => ({
    kind: 'merge' as const,
    key: `merge-${merge.repoPath}-${merge.hash}`,
    repoName: merge.repoName,
    repoPath: merge.repoPath,
    date: merge.mergedAt,
    subject: merge.subject,
    sourceBranch: merge.sourceBranch,
    targetBranch: merge.targetBranch,
  }))

  return [...commitRows, ...mergeRows].sort((left, right) => right.date.localeCompare(left.date))
}

function buildProjectSummaryRows(commits: CommitRecord[], mergedBranches: MergedBranchRecord[]) {
  const grouped = new Map<
    string,
    {
      repoName: string
      repoPath: string
      totalCommits: number
      totalAdditions: number
      totalDeletions: number
      mergedBranchCount: number
      summaries: string[]
    }
  >()

  for (const commit of commits) {
    const row = getProjectSummaryRow(grouped, commit.repoPath, commit.repoName)
    row.totalCommits += 1
    row.totalAdditions += commit.additions ?? 0
    row.totalDeletions += commit.deletions ?? 0
    row.summaries.push(buildCommitWorkSource(commit))
  }

  for (const merge of mergedBranches) {
    const row = getProjectSummaryRow(grouped, merge.repoPath, merge.repoName)
    row.mergedBranchCount += 1
    row.summaries.push(merge.sourceBranch ? `${merge.sourceBranch} 合并 ${merge.targetBranch}` : merge.subject)
  }

  return Array.from(grouped.values())
    .map((row) => {
      const changeSummary = summarizeSummaryItems(row.summaries)
      const aiSummary = getProjectAiSummary(row.repoPath)
      return {
        repoName: row.repoName,
        repoPath: row.repoPath,
        totalCommits: row.totalCommits,
        totalAdditions: row.totalAdditions,
        totalDeletions: row.totalDeletions,
        mergedBranchCount: row.mergedBranchCount,
        changeSummary,
        aiSummary,
        workSummary: aiSummary || getProjectAiSummaryPlaceholder(),
      }
    })
    .sort((left, right) => right.totalCommits - left.totalCommits || left.repoName.localeCompare(right.repoName))
}

function getProjectAiSummaryPlaceholder() {
  if (projectAiSummarizing.value) return 'DeepSeek 正在分析真实代码改动…'
  if (projectAiSummaryError.value) return projectAiSummaryError.value
  return '等待 DeepSeek 分析真实代码改动'
}

function getProjectSummaryRow(
  grouped: Map<
    string,
    {
      repoName: string
      repoPath: string
      totalCommits: number
      totalAdditions: number
      totalDeletions: number
      mergedBranchCount: number
      summaries: string[]
    }
  >,
  repoPath: string,
  repoName: string,
) {
  const existing = grouped.get(repoPath)
  if (existing) return existing

  const row = {
    repoName,
    repoPath,
    totalCommits: 0,
    totalAdditions: 0,
    totalDeletions: 0,
    mergedBranchCount: 0,
    summaries: [],
  }
  grouped.set(repoPath, row)
  return row
}

function buildCommitWorkSource(commit: CommitRecord) {
  const fileSummary = summarizeCommitFiles(commit)
  if (fileSummary !== '-') return fileSummary
  return normalizeSubject(commit.subject)
}

function summarizeCommitFiles(commit: CommitRecord, limit = MAX_PROJECT_SUMMARY_ITEMS) {
  const files = getCommitFiles(commit)
  if (!files.length) return '-'

  const areas = summarizeFileAreas(files, limit)
  const hiddenCount = Math.max(0, files.length - areas.sourceCount)
  return hiddenCount > 0 ? `${areas.labels.join('、')} 等 ${hiddenCount} 个文件` : areas.labels.join('、')
}

function summarizeFileAreas(files: CommitFileChange[], limit: number) {
  const seen = new Set<string>()
  const labels: string[] = []
  const sortedFiles = sortCommitFilesByChangeSize(files)

  for (const file of sortedFiles) {
    const label = getFileAreaLabel(file.path)
    if (!label || seen.has(label)) continue
    seen.add(label)
    labels.push(label)
    if (labels.length >= limit) break
  }

  return {
    labels: labels.length ? labels : ['文件改动'],
    sourceCount: Math.min(files.length, seen.size),
  }
}

function getCommitFiles(commit: CommitRecord) {
  return Array.isArray(commit.files) ? commit.files.filter((file) => file.path) : []
}

function sortCommitFilesByChangeSize(files: CommitFileChange[]) {
  return [...files].sort((left, right) => getFileChangeSize(right) - getFileChangeSize(left) || left.path.localeCompare(right.path))
}

function getFileChangeSize(file: CommitFileChange) {
  return (file.additions ?? 0) + (file.deletions ?? 0)
}

function getFileAreaLabel(path: string) {
  const normalizedPath = path.replace(/\\/g, '/').replace(/^"+|"+$/g, '').trim()
  if (!normalizedPath) return ''

  const parts = normalizedPath.split('/').filter(Boolean)
  if (parts.length <= 1) return normalizedPath
  if (parts[0] === 'src' && parts.length >= 3) return parts.slice(0, 3).join('/')
  if (parts[0].startsWith('.')) return parts.slice(0, Math.min(2, parts.length)).join('/')
  return parts.slice(0, Math.min(2, parts.length)).join('/')
}

function compactPath(path: string) {
  const normalizedPath = path.replace(/\\/g, '/').replace(/^"+|"+$/g, '').trim()
  const parts = normalizedPath.split('/').filter(Boolean)
  if (parts.length <= 2) return normalizedPath
  return `${parts[0]}/.../${parts[parts.length - 1]}`
}

function summarizeSummaryItems(items: string[]) {
  const seen = new Set<string>()
  const summaryItems: string[] = []

  for (const item of items) {
    const normalized = normalizeSummaryItem(item)
    if (!normalized || seen.has(normalized)) continue
    seen.add(normalized)
    summaryItems.push(normalized)
    if (summaryItems.length >= MAX_PROJECT_SUMMARY_ITEMS) break
  }

  return summaryItems.length ? summaryItems.join('、') : '-'
}

function normalizeSummaryItem(value: string) {
  return value
    .replace(/\s+/g, ' ')
    .replace(/[。；;,.，、]+$/g, '')
    .trim()
}

function normalizeSubject(subject: string) {
  return subject
    .replace(/^\s*(feat|fix|chore|docs|style|refactor|test|perf|build|ci)(\([^)]+\))?\s*[:：]\s*/i, '')
    .replace(/\s+/g, ' ')
    .replace(/[。；;,.，、]+$/g, '')
    .trim()
}

function escapeMarkdownCell(value: string) {
  return value.replace(/\|/g, '\\|').replace(/\n/g, ' ')
}

function normalizeEmail(value: string) {
  return value.trim().toLowerCase()
}

function createEmptyConfig(): AppConfig {
  const defaultRange = getDefaultDateRange()
  const aiModels = createDefaultAiModels()
  const aiSkills = createDefaultAiSkills()
  return {
    scanDirs: [],
    emails: [],
    autostart: false,
    targetBranch: DEFAULT_TARGET_BRANCH,
    dateRangeStart: defaultRange.start,
    dateRangeEnd: defaultRange.end,
    aiModels,
    defaultAiModelId: aiModels[0]?.id ?? '',
    aiSkills,
  }
}

function createDefaultAiModels(): AiModelConfig[] {
  return [
    {
      id: 'openai-gpt-4o',
      name: 'OpenAI GPT-4o',
      provider: 'OpenAI',
      model: 'gpt-4o',
      baseUrl: 'https://api.openai.com/v1',
      apiKey: '',
    },
    {
      id: 'qwen-3-5',
      name: '通义千问 3.5',
      provider: 'DashScope',
      model: 'qwen-plus',
      baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
      apiKey: '',
    },
    {
      id: 'deepseek-chat',
      name: 'DeepSeek Chat',
      provider: 'DeepSeek',
      model: 'deepseek-chat',
      baseUrl: 'https://api.deepseek.com',
      apiKey: '',
    },
  ]
}

function createBlankAiModel(): AiModelConfig {
  return {
    id: createId('model'),
    name: '',
    provider: '',
    model: '',
    baseUrl: '',
    apiKey: '',
  }
}

function createDefaultAiSkills(): AiSkillConfig[] {
  return DEFAULT_AI_SKILLS.map((skill) => ({ ...skill }))
}

function normalizeAiModels(models: AiModelConfig[] | undefined) {
  const normalizedModels = (Array.isArray(models) ? models : createDefaultAiModels())
    .map(normalizeAiModel)
    .filter((model) => model.id && model.name)
  const existingIds = new Set(normalizedModels.map((model) => model.id))
  const missingDefaultModels = createDefaultAiModels().filter((model) => !existingIds.has(model.id))
  return [...normalizedModels, ...missingDefaultModels]
}

function normalizeAiModel(model: AiModelConfig): AiModelConfig {
  return {
    id: (model.id || createId('model')).trim(),
    name: (model.name ?? '').trim(),
    provider: (model.provider ?? '').trim(),
    model: (model.model ?? '').trim(),
    baseUrl: (model.baseUrl ?? '').trim(),
    apiKey: (model.apiKey ?? '').trim(),
  }
}

function normalizeDefaultAiModelId(defaultId: string | undefined, models: AiModelConfig[]) {
  if (defaultId && models.some((model) => model.id === defaultId)) return defaultId
  return models[0]?.id ?? ''
}

function normalizeAiSkills(skills: AiSkillConfig[] | undefined) {
  const incomingSkills = new Map((Array.isArray(skills) ? skills : []).map((skill) => [skill.id, skill]))
  return DEFAULT_AI_SKILLS.map((defaultSkill) => normalizeAiSkill({ ...defaultSkill, ...(incomingSkills.get(defaultSkill.id) ?? {}) }))
}

function normalizeAiSkill(skill: AiSkillConfig): AiSkillConfig {
  const defaultSkill = getDefaultAiSkillConfig(skill.id) ?? DEFAULT_AI_SKILLS[0]
  return {
    id: defaultSkill.id,
    label: (skill.label || defaultSkill.label).trim(),
    description: (skill.description || defaultSkill.description).trim(),
    systemPrompt: (skill.systemPrompt || defaultSkill.systemPrompt).trim(),
    taskPrompt: (skill.taskPrompt || defaultSkill.taskPrompt).trim(),
    outputFormat: (skill.outputFormat || defaultSkill.outputFormat).trim(),
    contextPolicy: normalizeAiSkillContextPolicy(skill.contextPolicy, defaultSkill.contextPolicy),
    temperature: normalizeAiSkillTemperature(skill.temperature, defaultSkill.temperature),
  }
}

function normalizeAiSkillContextPolicy(value: string | undefined, fallback: AiSkillContextPolicy): AiSkillContextPolicy {
  return AI_SKILL_CONTEXT_OPTIONS.some((option) => option.value === value) ? value as AiSkillContextPolicy : fallback
}

function normalizeAiSkillTemperature(value: number | undefined, fallback: number) {
  const numberValue = Number(value)
  if (!Number.isFinite(numberValue)) return fallback
  return Math.min(1, Math.max(0, Number(numberValue.toFixed(2))))
}

function getDefaultAiSkillConfig(skillId: string) {
  return DEFAULT_AI_SKILLS.find((skill) => skill.id === skillId) ?? null
}

function getAiSkillConfig(skillId: string) {
  return config.value.aiSkills.find((skill) => skill.id === skillId) ?? getDefaultAiSkillConfig(skillId)
}

function getAiSkillIcon(skillId: string) {
  return AI_SKILLS.find((skill) => skill.id === skillId)?.icon ?? Bot
}

function getAiSkillContextLabel(value: AiSkillContextPolicy) {
  return AI_SKILL_CONTEXT_OPTIONS.find((option) => option.value === value)?.label ?? '可选 Git 上下文'
}

function normalizeTargetBranch(value: string | undefined) {
  const branch = (value ?? '').trim().replace(/^refs\/heads\//, '').replace(/^origin\//, '').replace(/^\/+|\/+$/g, '')
  return branch || DEFAULT_TARGET_BRANCH
}

function getDefaultDateRange() {
  const now = new Date()
  const start = new Date(now)
  const daysFromMonday = (start.getDay() + 6) % 7
  start.setDate(start.getDate() - daysFromMonday)
  return {
    start: formatDateInput(start),
    end: formatDateInput(now),
  }
}

function getCurrentMonthDateRange() {
  const now = new Date()
  const start = new Date(now.getFullYear(), now.getMonth(), 1)
  return {
    start: formatDateInput(start),
    end: formatDateInput(now),
  }
}

function getTrailingDateRange(dayCount: number) {
  const now = new Date()
  const start = new Date(now)
  start.setDate(now.getDate() - Math.max(dayCount - 1, 0))
  return {
    start: formatDateInput(start),
    end: formatDateInput(now),
  }
}

function getQuickDateRange(value: string) {
  if (value === 'this-week') return getDefaultDateRange()
  if (value === 'this-month') return getCurrentMonthDateRange()
  if (value === 'today') return getTrailingDateRange(1)
  if (value === 'last-3-days') return getTrailingDateRange(3)
  if (value === 'last-7-days') return getTrailingDateRange(7)
  if (value === 'last-14-days') return getTrailingDateRange(14)
  if (value === 'last-30-days') return getTrailingDateRange(30)
  if (value === 'last-90-days') return getTrailingDateRange(90)
  return null
}

function findMatchingQuickRange(start: string, end: string) {
  for (const option of QUICK_RANGE_OPTIONS) {
    const range = getQuickDateRange(option.value)
    if (range?.start === start && range.end === end) return option.value
  }
  return CUSTOM_QUICK_RANGE
}

function normalizeDateRangeValues(start: string | undefined, end: string | undefined) {
  const defaultRange = getDefaultDateRange()
  const startValue = (start ?? '').trim()
  const endValue = (end ?? '').trim()
  const nextStart = isDateInput(startValue) ? startValue : defaultRange.start
  const nextEnd = isDateInput(endValue) ? endValue : defaultRange.end

  if (nextStart > nextEnd) {
    return {
      start: nextStart,
      end: nextEnd,
      error: '开始日期不能晚于结束日期',
    }
  }

  return {
    start: nextStart,
    end: nextEnd,
    error: '',
  }
}

function isDateInput(value: string) {
  if (!value || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false
  const [year, month, day] = value.split('-').map(Number)
  const date = new Date(year, month - 1, day)
  return date.getFullYear() === year && date.getMonth() === month - 1 && date.getDate() === day
}

function formatDateInput(date: Date) {
  const year = date.getFullYear()
  const month = `${date.getMonth() + 1}`.padStart(2, '0')
  const day = `${date.getDate()}`.padStart(2, '0')
  return `${year}-${month}-${day}`
}

function formatDateDisplay(value: string) {
  return isDateInput(value) ? value.replace(/-/g, '/') : '-'
}

function parseDateInput(value: string) {
  if (!isDateInput(value)) return null
  const [year, month, day] = value.split('-').map(Number)
  return new Date(year, month - 1, day)
}

function startOfMonth(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), 1)
}

function buildCalendarDays(cursor: Date, startValue: string, endValue: string) {
  const monthStart = startOfMonth(cursor)
  const gridStart = new Date(monthStart)
  gridStart.setDate(monthStart.getDate() - monthStart.getDay())
  const today = formatDateInput(new Date())
  const rangeEnd = endValue || startValue
  const days: CalendarDay[] = []

  for (let index = 0; index < 42; index += 1) {
    const date = new Date(gridStart)
    date.setDate(gridStart.getDate() + index)
    const value = formatDateInput(date)
    days.push({
      key: value,
      value,
      label: date.getDate(),
      inMonth: date.getMonth() === cursor.getMonth(),
      isToday: value === today,
      isStart: value === startValue,
      isEnd: Boolean(rangeEnd) && value === rangeEnd,
      inRange: Boolean(startValue && rangeEnd && value > startValue && value < rangeEnd),
    })
  }

  return days
}

function formatDate(value: string) {
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  })
}

function showToast(message: string) {
  toast.value = message
  window.setTimeout(() => {
    if (toast.value === message) toast.value = ''
  }, 1500)
}

function readableError(error: unknown) {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return '操作失败'
}

function createId(prefix: string) {
  if (crypto.randomUUID) return `${prefix}-${crypto.randomUUID()}`
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2)}`
}
</script>

<template>
  <main class="relative h-screen w-screen overflow-hidden bg-white text-[13px] leading-[1.5] text-textMain shadow-popup">
    <section v-if="activeView === 'home'" class="flex h-full flex-col bg-panel">
      <header class="shrink-0 border-b border-line bg-white px-4 py-3">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="flex min-w-0 items-center gap-2">
              <h1 class="truncate text-[16px] font-semibold text-textMain">GitSage</h1>
              <span
                class="shrink-0 rounded-[4px] px-2 py-0.5 text-[11px] font-medium"
                :class="report ? 'bg-blue-50 text-primary' : 'bg-panel text-textMuted'"
              >
                {{ loading ? '扫描中' : report ? '已更新' : '待配置' }}
              </span>
            </div>
            <p class="mt-0.5 truncate text-[12px] text-textMuted">
              {{ report ? `更新于 ${formatDate(report.scannedAt)}` : hasScanDirs ? '选择范围后生成本地工作复盘' : '添加代码目录后开始扫描' }}
            </p>
          </div>

          <div class="flex shrink-0 items-center gap-2">
            <button
              class="inline-flex h-9 items-center justify-center gap-1.5 rounded-[6px] bg-primary px-3 text-[13px] font-medium text-white hover:bg-blue-600"
              @click="toggleAiAssistant"
            >
              <Bot class="h-4 w-4" />
              <span>AI助手</span>
            </button>
            <button
              class="inline-flex h-9 items-center justify-center gap-1.5 rounded-[6px] border border-line bg-white px-3 text-[13px] font-medium text-textMain hover:bg-panel disabled:cursor-not-allowed disabled:opacity-50"
              :disabled="!canCopy"
              title="复制周报"
              @click="copyWeeklyReport"
            >
              <Copy class="h-4 w-4" />
              <span>复制</span>
            </button>
            <button
              class="grid h-9 w-9 place-items-center rounded-[6px] border border-line bg-white text-textMuted hover:bg-panel hover:text-textMain disabled:cursor-not-allowed disabled:opacity-50"
              :disabled="loading || !hasScanDirs || !hasEmails"
              title="重新扫描"
              @click="refreshReport"
            >
              <Loader2 v-if="loading" class="h-4 w-4 animate-spin" />
              <RefreshCw v-else class="h-4 w-4" />
            </button>
            <button
              class="grid h-9 w-9 place-items-center rounded-[6px] border border-line bg-white text-textMuted hover:bg-panel hover:text-textMain"
              title="设置"
              @click="activeView = 'settings'"
            >
              <Settings class="h-4 w-4" />
            </button>
          </div>
        </div>
      </header>

      <div class="shrink-0 bg-white px-4 pb-3 pt-3">
        <div class="grid grid-cols-5 gap-2">
          <div
            v-for="item in summaryItems"
            :key="item.label"
            class="min-w-0 rounded-[6px] border border-line bg-white px-3 py-2 shadow-soft"
          >
            <div class="truncate text-[12px] text-textMuted" :title="item.label">{{ item.label }}</div>
            <div class="mt-0.5 truncate text-[20px] font-semibold leading-tight" :class="item.tone">{{ item.value }}</div>
          </div>
        </div>
      </div>

      <div class="shrink-0 border-y border-line bg-white px-4 py-3">
        <div class="mb-2 flex items-center justify-between gap-3">
          <div class="min-w-0">
            <h2 class="text-[13px] font-semibold text-textMain">工作范围</h2>
            <p class="truncate text-[12px] text-textMuted">
              {{ currentProjectName || '全部项目' }} · {{ dateRangeDisplay }} · {{ displayTargetBranch }}
            </p>
          </div>
          <span class="shrink-0 rounded-[4px] bg-panel px-2 py-1 text-[12px] text-textMuted">
            {{ visibleRowCount }} {{ visibleRowUnit }}
          </span>
        </div>

        <div class="grid grid-cols-[minmax(0,1.2fr)_120px_minmax(0,1fr)] gap-2">
          <select
            v-model="selectedRepoPath"
            class="h-9 min-w-0 rounded-[6px] border border-line bg-white px-3 text-[13px] text-textMain outline-none focus:border-primary disabled:text-textMuted"
            title="项目筛选"
            :disabled="!projectOptions.length"
            @change="closeProjectDetail"
          >
            <option :value="ALL_PROJECTS">全部项目</option>
            <option v-for="project in projectOptions" :key="project.path" :value="project.path">
              {{ project.name }}
            </option>
          </select>
          <select
            :value="activeQuickRange"
            class="h-9 rounded-[6px] border border-line bg-white px-3 text-[13px] font-medium text-textMain outline-none focus:border-primary"
            title="快捷时间范围"
            @change="handleQuickRangeChange"
          >
            <option :value="CUSTOM_QUICK_RANGE">自定义</option>
            <option v-for="option in QUICK_RANGE_OPTIONS" :key="option.value" :value="option.value">
              {{ option.label }}
            </option>
          </select>
          <div ref="datePickerRef" class="relative min-w-0">
            <button
              class="flex h-9 w-full min-w-0 items-center justify-between gap-2 rounded-[6px] border border-line bg-white px-3 text-left text-[13px] text-textMain outline-none hover:bg-panel focus:border-primary"
              title="选择日期范围"
              @click.stop="toggleDatePicker"
            >
              <span class="inline-flex min-w-0 items-center gap-2">
                <Calendar class="h-4 w-4 shrink-0 text-textMuted" />
                <span class="truncate">{{ dateRangeDisplay }}</span>
              </span>
              <ChevronDown class="h-4 w-4 shrink-0 text-textMuted" />
            </button>

            <transition enter-active-class="transition duration-150" enter-from-class="translate-y-1 opacity-0" enter-to-class="translate-y-0 opacity-100" leave-active-class="transition duration-100" leave-from-class="translate-y-0 opacity-100" leave-to-class="translate-y-1 opacity-0">
              <div v-if="datePickerOpen" class="absolute right-0 top-10 z-30 w-[368px] rounded-[6px] border border-line bg-white p-3 shadow-popup">
                <div class="mb-3 flex items-center justify-between">
                  <button class="grid h-7 w-7 place-items-center rounded-[6px] text-textMuted hover:bg-panel hover:text-textMain" title="上个月" @click="shiftCalendarMonth(-1)">
                    <ChevronLeft class="h-4 w-4" />
                  </button>
                  <div class="text-[13px] font-semibold">{{ calendarTitle }}</div>
                  <button class="grid h-7 w-7 place-items-center rounded-[6px] text-textMuted hover:bg-panel hover:text-textMain" title="下个月" @click="shiftCalendarMonth(1)">
                    <ChevronRight class="h-4 w-4" />
                  </button>
                </div>
                <div class="grid grid-cols-7 border-b border-line pb-2 text-center text-[12px] font-medium text-textMuted">
                  <span>日</span>
                  <span>一</span>
                  <span>二</span>
                  <span>三</span>
                  <span>四</span>
                  <span>五</span>
                  <span>六</span>
                </div>
                <div class="mt-2 grid grid-cols-7 justify-items-center gap-y-1">
                  <button
                    v-for="day in calendarDays"
                    :key="day.key"
                    :class="getCalendarDayClass(day)"
                    @click="selectCalendarDate(day)"
                  >
                    {{ day.label }}
                  </button>
                </div>
                <div class="mt-3 flex items-center justify-between border-t border-line pt-3">
                  <span class="min-w-0 truncate text-[12px] text-textMuted">{{ pendingDateRangeDisplay }}</span>
                  <div class="flex shrink-0 items-center gap-2">
                    <button class="h-8 rounded-[6px] border border-line bg-white px-3 text-[12px] font-medium text-textMain hover:bg-panel" @click="datePickerOpen = false">
                      取消
                    </button>
                    <button class="h-8 rounded-[6px] bg-primary px-3 text-[12px] font-medium text-white hover:bg-blue-600" @click="applyPendingDateRange">
                      应用
                    </button>
                  </div>
                </div>
              </div>
            </transition>
          </div>
        </div>

        <div class="mt-3 flex items-center justify-between rounded-[6px] bg-panel px-3 py-2">
          <span class="text-[12px] font-medium text-textMain">默认展示项目汇总</span>
          <span class="text-[12px] text-textMuted">点击项目进入明细</span>
        </div>
      </div>

      <div v-if="errorText" class="mx-4 mt-3 rounded-[6px] border border-red-100 bg-red-50 px-3 py-2 text-[12px] text-loss">
        {{ errorText }}
      </div>

      <section class="min-h-0 flex-1 overflow-hidden bg-white">
        <div v-if="loading && !visibleRowCount" class="grid h-full place-items-center text-[12px] text-textMuted">
          <div class="flex items-center gap-2">
            <Loader2 class="h-4 w-4 animate-spin text-primary" />
            <span>正在扫描</span>
          </div>
        </div>

        <div v-else-if="emptyText" class="grid h-full place-items-center px-8 text-center">
          <div class="w-full max-w-[430px]">
            <div class="mx-auto grid h-12 w-12 place-items-center rounded-full bg-blue-50 text-primary">
              <FolderPlus v-if="!hasScanDirs" class="h-5 w-5" />
              <Mail v-else-if="!hasEmails" class="h-5 w-5" />
              <Search v-else class="h-5 w-5" />
            </div>
            <h2 class="mt-3 text-[15px] font-semibold text-textMain">
              {{ !hasScanDirs ? '添加代码目录后开始复盘' : !hasEmails ? '配置 Git 邮箱后开始扫描' : '当前范围暂无记录' }}
            </h2>
            <p class="mx-auto mt-1 max-w-[360px] text-[13px] text-textMuted">
              {{ emptyText }}
            </p>
            <div class="mt-4 flex items-center justify-center gap-2">
              <button
                v-if="!hasScanDirs || !hasEmails"
                class="inline-flex h-9 items-center justify-center gap-1.5 rounded-[6px] bg-primary px-3 text-[13px] font-medium text-white hover:bg-blue-600"
                @click="activeView = 'settings'"
              >
                <Settings class="h-4 w-4" />
                <span>去设置</span>
              </button>
              <button
                class="inline-flex h-9 items-center justify-center gap-1.5 rounded-[6px] border border-line bg-white px-3 text-[13px] font-medium text-textMain hover:bg-panel"
                @click="toggleAiAssistant"
              >
                <Bot class="h-4 w-4" />
                <span>打开AI助手</span>
              </button>
            </div>
          </div>
        </div>

        <div v-else-if="selectedProjectDetailPath" class="flex h-full flex-col bg-white">
          <div class="flex h-[54px] shrink-0 items-center justify-between gap-3 border-b border-line px-4">
            <div class="flex min-w-0 items-center gap-2">
              <button
                class="grid h-8 w-8 shrink-0 place-items-center rounded-[6px] text-textMuted hover:bg-panel hover:text-textMain"
                title="返回项目汇总"
                @click="closeProjectDetail"
              >
                <ArrowLeft class="h-4 w-4" />
              </button>
              <div class="min-w-0">
                <h2 class="truncate text-[14px] font-semibold text-textMain">{{ selectedProjectDetailName || '项目详情' }}</h2>
                <p class="truncate text-[12px] text-textMuted">{{ projectDetailRows.length }} 条记录 · {{ dateRangeDisplay }}</p>
              </div>
            </div>
            <button
              class="inline-flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-[6px] border border-line bg-white px-3 text-[12px] font-medium text-textMain hover:bg-panel"
              @click="copyWeeklyReport"
            >
              <Copy class="h-4 w-4" />
              <span>复制项目周报</span>
            </button>
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto">
            <article
              v-for="row in projectDetailRows"
              :key="row.key"
              class="border-b border-line px-4 py-3 hover:bg-panel"
            >
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                  <div class="flex min-w-0 items-center gap-2">
                    <span
                      class="shrink-0 rounded-[4px] px-1.5 py-0.5 text-[11px] font-medium"
                      :class="row.kind === 'merge' ? 'bg-blue-50 text-primary' : 'bg-panel text-textMuted'"
                    >
                      {{ row.kind === 'merge' ? '已合' : '提交' }}
                    </span>
                    <span class="shrink-0 text-[12px] text-textMuted">{{ formatDate(row.date) }}</span>
                  </div>
                  <p v-if="row.kind === 'merge'" class="mt-1 truncate text-[12px] text-textMuted">
                    {{ row.sourceBranch ?? '未知分支' }} -> {{ row.targetBranch }}
                  </p>
                  <p class="mt-1 line-clamp-2 text-[13px] text-textMain">{{ row.subject }}</p>
                  <p v-if="row.kind === 'commit' && row.fileSummary !== '-'" class="mt-1 line-clamp-1 text-[12px] text-textMuted">
                    改动：{{ row.fileSummary }}
                  </p>
                  <div v-if="row.kind === 'commit' && row.files.length" class="mt-2 flex max-w-full flex-wrap gap-1">
                    <span
                      v-for="file in row.files.slice(0, MAX_COMMIT_FILE_CHIPS)"
                      :key="file.path"
                      class="max-w-[180px] truncate rounded-[4px] border border-line bg-panel px-1.5 py-0.5 text-[11px] text-textMuted"
                      :title="formatFileChangeForPrompt(file)"
                    >
                      {{ compactPath(file.path) }}
                    </span>
                    <span
                      v-if="row.files.length > MAX_COMMIT_FILE_CHIPS"
                      class="rounded-[4px] border border-line bg-panel px-1.5 py-0.5 text-[11px] text-textMuted"
                    >
                      +{{ row.files.length - MAX_COMMIT_FILE_CHIPS }}
                    </span>
                  </div>
                </div>
                <div v-if="row.kind === 'commit'" class="flex shrink-0 flex-col items-end gap-2 text-right text-[12px]">
                  <div>
                    <div class="font-medium text-gain">+{{ row.additions ?? '-' }}</div>
                    <div class="font-medium text-loss">-{{ row.deletions ?? '-' }}</div>
                  </div>
                  <button
                    class="inline-flex h-7 items-center justify-center gap-1 rounded-[6px] border border-line bg-white px-2 text-[11px] font-medium text-textMuted hover:bg-panel hover:text-textMain"
                    :title="isCommitDiffOpen(row.key) ? '收起 diff' : '查看 diff'"
                    @click.stop="toggleCommitDiff(row)"
                  >
                    <Loader2 v-if="isCommitDiffLoading(row.key)" class="h-3.5 w-3.5 animate-spin text-primary" />
                    <ChevronDown v-else-if="isCommitDiffOpen(row.key)" class="h-3.5 w-3.5" />
                    <ChevronRight v-else class="h-3.5 w-3.5" />
                    <span>{{ isCommitDiffOpen(row.key) ? '收起' : 'diff' }}</span>
                  </button>
                </div>
                <div v-else class="shrink-0 text-right text-[12px] font-medium text-primary">
                  {{ row.targetBranch }}
                </div>
              </div>
              <div
                v-if="row.kind === 'commit' && isCommitDiffOpen(row.key)"
                class="mt-3 overflow-hidden rounded-[6px] border border-line bg-white"
              >
                <div v-if="isCommitDiffLoading(row.key)" class="flex items-center gap-2 bg-panel px-3 py-2 text-[12px] text-textMuted">
                  <Loader2 class="h-4 w-4 animate-spin text-primary" />
                  <span>正在读取 diff</span>
                </div>
                <div v-else-if="getCommitDiffError(row.key)" class="bg-red-50 px-3 py-2 text-[12px] text-loss">
                  {{ getCommitDiffError(row.key) }}
                </div>
                <div v-else-if="!(getCommitDiffSummary(row.key)?.files.length)" class="bg-panel px-3 py-2 text-[12px] text-textMuted">
                  该提交没有可展示的文本 diff
                </div>
                <div v-else class="max-h-[360px] overflow-auto bg-[#fbfcfe]">
                  <div class="sticky top-0 z-20 flex items-center justify-between gap-3 border-b border-line bg-white px-3 py-2">
                    <div class="min-w-0">
                      <div class="text-[12px] font-semibold text-textMain">Diff 明细</div>
                      <div class="truncate text-[11px] text-textMuted">{{ getCommitDiffSummary(row.key)?.files.length }} 个文件</div>
                    </div>
                    <button
                      class="inline-flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-[6px] border border-line bg-white px-3 text-[12px] font-medium text-textMain hover:bg-panel disabled:cursor-not-allowed disabled:opacity-60"
                      :disabled="isCommitImpactLoading(row.key)"
                      title="使用 DeepSeek 分析影响范围和风险点"
                      @click.stop="analyzeCommitImpact(row)"
                    >
                      <Loader2 v-if="isCommitImpactLoading(row.key)" class="h-4 w-4 animate-spin text-primary" />
                      <Search v-else class="h-4 w-4" />
                      <span>{{ getCommitImpactAnalysis(row.key) ? '重新分析影响' : '查看影响范围' }}</span>
                    </button>
                  </div>
                  <div
                    v-if="isCommitImpactLoading(row.key) || getCommitImpactError(row.key) || getCommitImpactAnalysis(row.key)"
                    class="border-b border-line bg-white px-3 py-3"
                  >
                    <div v-if="isCommitImpactLoading(row.key)" class="flex items-center gap-2 text-[12px] text-textMuted">
                      <Loader2 class="h-4 w-4 animate-spin text-primary" />
                      <span>正在分析影响范围</span>
                    </div>
                    <div v-else-if="getCommitImpactError(row.key)" class="rounded-[6px] bg-red-50 px-3 py-2 text-[12px] text-loss">
                      {{ getCommitImpactError(row.key) }}
                    </div>
                    <div v-else-if="getCommitImpactAnalysis(row.key)" class="space-y-3 text-[12px]">
                      <div class="rounded-[6px] bg-panel px-3 py-2 text-textMain">
                        <span class="font-semibold">影响范围：</span>{{ getCommitImpactAnalysis(row.key)?.scope || '影响范围不明确' }}
                      </div>
                      <div class="grid gap-2 md:grid-cols-3">
                        <div class="rounded-[6px] border border-red-100 bg-red-50 p-3">
                          <div class="mb-1 font-semibold text-loss">高风险</div>
                          <p v-if="!getCommitImpactAnalysis(row.key)?.high.length" class="text-textMuted">暂无明显高风险</p>
                          <ul v-else class="list-disc space-y-1 pl-4 text-red-800">
                            <li v-for="item in getCommitImpactAnalysis(row.key)?.high" :key="item">{{ item }}</li>
                          </ul>
                        </div>
                        <div class="rounded-[6px] border border-yellow-100 bg-yellow-50 p-3">
                          <div class="mb-1 font-semibold text-yellow-700">中风险</div>
                          <p v-if="!getCommitImpactAnalysis(row.key)?.medium.length" class="text-textMuted">暂无明显中风险</p>
                          <ul v-else class="list-disc space-y-1 pl-4 text-yellow-800">
                            <li v-for="item in getCommitImpactAnalysis(row.key)?.medium" :key="item">{{ item }}</li>
                          </ul>
                        </div>
                        <div class="rounded-[6px] border border-blue-100 bg-blue-50 p-3">
                          <div class="mb-1 font-semibold text-primary">低风险</div>
                          <p v-if="!getCommitImpactAnalysis(row.key)?.low.length" class="text-textMuted">暂无明显低风险</p>
                          <ul v-else class="list-disc space-y-1 pl-4 text-blue-800">
                            <li v-for="item in getCommitImpactAnalysis(row.key)?.low" :key="item">{{ item }}</li>
                          </ul>
                        </div>
                      </div>
                      <div v-if="getCommitImpactAnalysis(row.key)?.verification.length" class="rounded-[6px] border border-line bg-white p-3">
                        <div class="mb-1 font-semibold text-textMain">验证建议</div>
                        <ul class="list-disc space-y-1 pl-4 text-textMuted">
                          <li v-for="item in getCommitImpactAnalysis(row.key)?.verification" :key="item">{{ item }}</li>
                        </ul>
                      </div>
                    </div>
                  </div>
                  <div
                    v-for="file in getCommitDiffSummary(row.key)?.files"
                    :key="file.path"
                    class="border-b border-line last:border-b-0"
                  >
                    <div class="sticky top-0 z-10 border-b border-line bg-panel px-3 py-2 font-mono text-[12px] font-semibold text-textMain">
                      {{ file.path }}
                    </div>
                    <div class="py-1">
                      <div
                        v-for="(line, index) in file.lines"
                        :key="`${file.path}-${index}`"
                        class="grid grid-cols-[28px_1fr] gap-2 px-3 py-0.5 font-mono text-[12px] leading-5"
                        :class="getDiffLineClass(line.kind)"
                      >
                        <span class="select-none text-right opacity-70">{{ getDiffLinePrefix(line.kind) }}</span>
                        <span class="whitespace-pre-wrap break-all">{{ line.content || ' ' }}</span>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </article>
          </div>
        </div>

        <div v-else class="h-full overflow-y-auto p-4">
          <div class="mb-3 flex items-center justify-between gap-3">
            <div class="min-w-0">
              <h2 class="text-[14px] font-semibold text-textMain">项目汇总</h2>
              <p class="truncate text-[12px] text-textMuted">按仓库聚合提交、代码变动和一句话功能摘要</p>
            </div>
            <div class="flex shrink-0 items-center gap-2">
              <button
                class="inline-flex h-8 items-center justify-center gap-1.5 rounded-[6px] border border-line bg-white px-3 text-[12px] font-medium text-textMain hover:bg-panel disabled:cursor-not-allowed disabled:opacity-60"
                :disabled="projectAiSummarizing || !projectSummaryRows.length"
                title="使用 DeepSeek 重新分析真实代码改动"
                @click="() => generateProjectAiSummaries()"
              >
                <Loader2 v-if="projectAiSummarizing" class="h-4 w-4 animate-spin text-primary" />
                <Bot v-else class="h-4 w-4" />
                <span>{{ projectAiSummarizing ? '分析中' : '重新分析' }}</span>
              </button>
              <span class="rounded-[4px] bg-panel px-2 py-1 text-[12px] text-textMuted">
                {{ projectSummaryRows.length }} 个项目
              </span>
            </div>
          </div>
          <div class="overflow-hidden rounded-[6px] border border-line bg-white">
            <table class="w-full table-fixed border-collapse text-left text-[12px]">
              <thead class="sticky top-0 z-10 bg-panel text-textMuted">
                <tr>
                  <th class="w-[150px] border-b border-r border-line px-3 py-2 font-semibold">项目</th>
                  <th class="w-[64px] border-b border-r border-line px-2 py-2 text-right font-semibold">提交</th>
                  <th class="w-[72px] border-b border-r border-line px-2 py-2 text-right font-semibold">新增</th>
                  <th class="w-[72px] border-b border-r border-line px-2 py-2 text-right font-semibold">删除</th>
                  <th class="w-[72px] border-b border-r border-line px-2 py-2 text-right font-semibold">已合</th>
                  <th class="border-b border-r border-line px-3 py-2 font-semibold">功能摘要</th>
                  <th class="w-[54px] border-b border-line px-2 py-2 text-center font-semibold">详情</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="row in projectSummaryRows"
                  :key="row.repoPath"
                  class="cursor-pointer hover:bg-blue-50/60"
                  @click="openProjectDetail(row.repoPath)"
                >
                  <td class="border-b border-r border-line px-3 py-3 align-top">
                    <span class="block truncate font-semibold text-primary" :title="row.repoName">{{ row.repoName }}</span>
                  </td>
                  <td class="border-b border-r border-line px-2 py-3 text-right align-top font-medium text-textMain">{{ row.totalCommits }}</td>
                  <td class="border-b border-r border-line px-2 py-3 text-right align-top font-medium text-gain">+{{ row.totalAdditions }}</td>
                  <td class="border-b border-r border-line px-2 py-3 text-right align-top font-medium text-loss">-{{ row.totalDeletions }}</td>
                  <td class="border-b border-r border-line px-2 py-3 text-right align-top font-medium text-primary">{{ row.mergedBranchCount }}</td>
                  <td class="border-b border-r border-line px-3 py-3 align-top text-textMain">
                    <div class="flex min-w-0 items-start gap-2">
                      <span
                        v-if="row.aiSummary"
                        class="mt-0.5 shrink-0 rounded-[4px] bg-blue-50 px-1.5 py-0.5 text-[11px] font-medium text-primary"
                      >
                        AI
                      </span>
                      <p
                        class="min-w-0 flex-1 line-clamp-2"
                        :class="row.aiSummary ? 'text-textMain' : 'text-textMuted'"
                        :title="row.aiSummary || row.changeSummary"
                      >
                        {{ row.workSummary }}
                      </p>
                      <button
                        class="grid h-7 w-7 shrink-0 place-items-center rounded-[6px] text-textMuted hover:bg-panel hover:text-textMain disabled:cursor-not-allowed disabled:opacity-40"
                        :disabled="!row.aiSummary"
                        title="复制功能摘要"
                        @click.stop="copyProjectSummary(row.aiSummary)"
                      >
                        <Copy class="h-4 w-4" />
                      </button>
                    </div>
                  </td>
                  <td class="border-b border-line px-2 py-3 text-center align-top text-textMuted">
                    <ChevronRight class="mx-auto h-4 w-4" />
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </section>

      <footer class="flex h-[38px] shrink-0 items-center justify-between border-t border-line bg-white px-4">
        <div class="min-w-0 truncate text-[12px] text-textMuted">
          {{ report?.issues.length ? `${report.issues.length} 条扫描提示` : '数据仅保存在本机' }}
        </div>
        <div class="ml-3 flex min-w-0 shrink-0 items-center gap-2 text-[12px] text-textMuted">
          <span class="truncate">{{ dateRangeDisplay }}</span>
          <span class="h-3 w-px bg-line" />
          <span class="truncate">{{ currentProjectName || '全部项目' }}</span>
        </div>
      </footer>
    </section>

    <section v-else class="flex h-full flex-col">
      <header class="flex h-[48px] shrink-0 items-center justify-between border-b border-line px-4">
        <button
          class="inline-flex h-8 w-8 items-center justify-center rounded-[6px] text-textMuted hover:bg-panel hover:text-textMain"
          title="返回"
          @click="activeView = 'home'"
        >
          <ArrowLeft class="h-4 w-4" />
        </button>
        <h1 class="text-[14px] font-semibold">设置</h1>
        <button
          class="inline-flex h-8 w-8 items-center justify-center rounded-[6px] text-textMuted hover:bg-panel hover:text-loss"
          title="重置配置"
          @click="resetConfig"
        >
          <RotateCcw class="h-4 w-4" />
        </button>
      </header>

      <div class="min-h-0 flex-1 overflow-y-auto px-4 py-4">
        <section class="mb-5">
          <div class="mb-2 flex items-center justify-between">
            <h2 class="text-[13px] font-semibold">Git 个人邮箱</h2>
            <span class="text-[12px] text-textMuted">{{ config.emails.length }}</span>
          </div>
          <div class="flex gap-2">
            <div class="relative min-w-0 flex-1">
              <Mail class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-textMuted" />
              <input
                v-model="emailDraft"
                class="h-9 w-full rounded-[6px] border border-line bg-white pl-9 pr-3 text-[13px] outline-none focus:border-primary"
                placeholder="name@example.com"
                @keydown.enter.prevent="addEmail"
              />
            </div>
            <button
              class="grid h-9 w-9 shrink-0 place-items-center rounded-[6px] bg-primary text-white hover:bg-blue-600"
              title="添加邮箱"
              @click="addEmail"
            >
              <Plus class="h-4 w-4" />
            </button>
          </div>
          <div v-if="config.emails.length" class="mt-2 flex flex-wrap gap-2">
            <span
              v-for="email in config.emails"
              :key="email"
              class="inline-flex h-7 max-w-full items-center gap-1 rounded-[6px] bg-panel px-2 text-[12px] text-textMain"
            >
              <span class="max-w-[330px] truncate">{{ email }}</span>
              <button class="text-textMuted hover:text-loss" title="删除邮箱" @click="removeEmail(email)">
                <X class="h-3.5 w-3.5" />
              </button>
            </span>
          </div>
        </section>

        <section class="mb-5">
          <div class="mb-2 flex items-center justify-between">
            <h2 class="text-[13px] font-semibold">扫描目录</h2>
            <button
              class="inline-flex h-7 items-center gap-1.5 rounded-[6px] px-2 text-[12px] text-primary hover:bg-panel"
              @click="addDirectory"
            >
              <FolderPlus class="h-4 w-4" />
              <span>添加</span>
            </button>
          </div>
          <div v-if="config.scanDirs.length" class="overflow-hidden rounded-[6px] border border-line">
            <div v-for="dir in config.scanDirs" :key="dir" class="flex items-center justify-between gap-2 border-b border-line px-3 py-2 last:border-b-0">
              <span class="min-w-0 flex-1 truncate text-[12px] text-textMain" :title="dir">{{ dir }}</span>
              <button class="grid h-7 w-7 place-items-center rounded-[6px] text-textMuted hover:bg-red-50 hover:text-loss" title="删除目录" @click="removeDirectory(dir)">
                <Trash2 class="h-4 w-4" />
              </button>
            </div>
          </div>
          <div v-else class="rounded-[6px] border border-dashed border-line px-3 py-4 text-center text-[12px] text-textMuted">
            请添加本地代码目录以开始扫描
          </div>
        </section>

        <section class="mb-5">
          <div class="mb-2 flex items-center justify-between">
            <h2 class="text-[13px] font-semibold">目标分支</h2>
            <span class="text-[12px] text-textMuted">{{ config.targetBranch }}</span>
          </div>
          <div class="relative">
            <GitBranch class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-textMuted" />
            <input
              v-model.trim="config.targetBranch"
              class="h-9 w-full rounded-[6px] border border-line bg-white pl-9 pr-3 text-[13px] outline-none focus:border-primary"
              placeholder="main"
              @blur="saveTargetBranch"
              @keydown.enter.prevent="saveTargetBranch"
            />
          </div>
        </section>

        <section class="mb-5">
          <div class="mb-2 flex items-center justify-between">
            <h2 class="text-[13px] font-semibold">AI模型管理</h2>
            <span class="text-[12px] text-textMuted">{{ aiModelSummary }}</span>
          </div>
          <div class="space-y-2">
            <div
              v-for="model in config.aiModels"
              :key="model.id"
              class="flex items-center justify-between gap-2 rounded-[6px] border border-line bg-white px-3 py-2"
            >
              <button class="flex min-w-0 flex-1 items-center gap-2 text-left" @click="openAiModelEditor(model)">
                <span
                  class="grid h-8 w-8 shrink-0 place-items-center rounded-full"
                  :class="model.id === config.defaultAiModelId ? 'bg-blue-100 text-primary' : 'bg-green-100 text-gain'"
                >
                  <Bot class="h-4 w-4" />
                </span>
                <span class="min-w-0">
                  <span class="block truncate text-[13px] font-medium text-textMain">{{ model.name }}</span>
                  <span class="block truncate text-[12px] text-textMuted">{{ model.provider || '自定义' }} / {{ model.model }}</span>
                </span>
              </button>
              <span v-if="model.id === config.defaultAiModelId" class="shrink-0 rounded-[4px] bg-blue-50 px-2 py-1 text-[11px] font-medium text-primary">
                默认
              </span>
              <button
                v-else
                class="shrink-0 rounded-[6px] px-2 py-1 text-[12px] text-textMuted hover:bg-panel hover:text-primary"
                title="设为默认"
                @click="setDefaultAiModel(model.id)"
              >
                默认
              </button>
              <button class="grid h-7 w-7 shrink-0 place-items-center rounded-[6px] text-textMuted hover:bg-panel hover:text-textMain" title="编辑模型" @click="openAiModelEditor(model)">
                <MoreVertical class="h-4 w-4" />
              </button>
              <button class="grid h-7 w-7 shrink-0 place-items-center rounded-[6px] text-textMuted hover:bg-red-50 hover:text-loss" title="删除模型" @click="removeAiModel(model.id)">
                <Trash2 class="h-4 w-4" />
              </button>
            </div>

            <button
              class="inline-flex h-9 w-full items-center justify-center gap-2 rounded-[6px] border border-line bg-white text-[13px] font-medium text-textMuted hover:bg-panel hover:text-textMain"
              @click="openAiModelEditor()"
            >
              <Plus class="h-4 w-4" />
              <span>添加模型</span>
            </button>
          </div>

          <div v-if="aiModelEditorOpen" class="mt-3 rounded-[6px] border border-line bg-panel p-3">
            <div class="mb-3 flex items-center justify-between">
              <h3 class="text-[13px] font-semibold">{{ editingAiModelId ? '编辑模型' : '添加模型' }}</h3>
              <button class="grid h-7 w-7 place-items-center rounded-[6px] text-textMuted hover:bg-white hover:text-textMain" title="关闭" @click="closeAiModelEditor">
                <X class="h-4 w-4" />
              </button>
            </div>
            <div class="grid gap-2">
              <input
                v-model.trim="aiModelDraft.name"
                class="h-9 rounded-[6px] border border-line bg-white px-3 text-[13px] outline-none focus:border-primary"
                placeholder="模型名称，如 OpenAI GPT-4o"
              />
              <div class="grid grid-cols-2 gap-2">
                <input
                  v-model.trim="aiModelDraft.provider"
                  class="h-9 min-w-0 rounded-[6px] border border-line bg-white px-3 text-[13px] outline-none focus:border-primary"
                  placeholder="服务商"
                />
                <input
                  v-model.trim="aiModelDraft.model"
                  class="h-9 min-w-0 rounded-[6px] border border-line bg-white px-3 text-[13px] outline-none focus:border-primary"
                  placeholder="模型ID"
                />
              </div>
              <input
                v-model.trim="aiModelDraft.baseUrl"
                class="h-9 rounded-[6px] border border-line bg-white px-3 text-[13px] outline-none focus:border-primary"
                placeholder="Base URL"
              />
              <input
                v-model.trim="aiModelDraft.apiKey"
                type="password"
                autocomplete="off"
                class="h-9 rounded-[6px] border border-line bg-white px-3 text-[13px] outline-none focus:border-primary"
                placeholder="API Key"
              />
              <div class="flex justify-end gap-2">
                <button class="h-8 rounded-[6px] border border-line bg-white px-3 text-[12px] font-medium text-textMain hover:bg-panel" @click="closeAiModelEditor">
                  取消
                </button>
                <button class="h-8 rounded-[6px] bg-primary px-3 text-[12px] font-medium text-white hover:bg-blue-600" @click="saveAiModelDraft">
                  保存模型
                </button>
              </div>
            </div>
          </div>
        </section>

        <section class="mb-5">
          <div class="mb-2 flex items-center justify-between gap-2">
            <div>
              <h2 class="text-[13px] font-semibold">AI技能管理</h2>
              <p class="text-[12px] text-textMuted">每个快捷技能可单独配置 Prompt、上下文和温度</p>
            </div>
            <div class="flex shrink-0 items-center gap-2">
              <span class="text-[12px] text-textMuted">{{ aiSkillSummary }}</span>
              <button
                class="h-7 rounded-[6px] px-2 text-[12px] text-textMuted hover:bg-panel hover:text-loss"
                @click="resetAllAiSkills"
              >
                重置全部
              </button>
            </div>
          </div>

          <div class="space-y-2">
            <div
              v-for="skill in config.aiSkills"
              :key="skill.id"
              class="rounded-[6px] border border-line bg-white"
            >
              <button
                class="flex w-full items-center justify-between gap-3 px-3 py-2 text-left"
                @click="openAiSkillEditor(skill.id)"
              >
                <span class="flex min-w-0 items-center gap-2">
                  <span
                    class="grid h-8 w-8 shrink-0 place-items-center rounded-full"
                    :class="editingAiSkillId === skill.id ? 'bg-blue-100 text-primary' : 'bg-panel text-textMuted'"
                  >
                    <component :is="getAiSkillIcon(skill.id)" class="h-4 w-4" />
                  </span>
                  <span class="min-w-0">
                    <span class="block truncate text-[13px] font-medium text-textMain">{{ skill.label }}</span>
                    <span class="block truncate text-[12px] text-textMuted">{{ skill.description }}</span>
                  </span>
                </span>
                <span class="shrink-0 rounded-[4px] bg-panel px-2 py-1 text-[11px] text-textMuted">
                  {{ getAiSkillContextLabel(skill.contextPolicy) }} / T {{ skill.temperature }}
                </span>
              </button>

              <div v-if="editingAiSkillId === skill.id" class="border-t border-line bg-panel p-3">
                <div class="grid gap-2">
                  <div class="grid grid-cols-2 gap-2">
                    <label class="grid gap-1">
                      <span class="text-[12px] font-medium text-textMuted">技能名称</span>
                      <input
                        v-model.trim="skill.label"
                        class="h-9 min-w-0 rounded-[6px] border border-line bg-white px-3 text-[13px] outline-none focus:border-primary"
                      />
                    </label>
                    <label class="grid gap-1">
                      <span class="text-[12px] font-medium text-textMuted">Git 上下文</span>
                      <select
                        v-model="skill.contextPolicy"
                        class="h-9 min-w-0 rounded-[6px] border border-line bg-white px-3 text-[13px] outline-none focus:border-primary"
                      >
                        <option v-for="option in AI_SKILL_CONTEXT_OPTIONS" :key="option.value" :value="option.value">
                          {{ option.label }}
                        </option>
                      </select>
                    </label>
                  </div>

                  <label class="grid gap-1">
                    <span class="text-[12px] font-medium text-textMuted">技能描述</span>
                    <input
                      v-model.trim="skill.description"
                      class="h-9 rounded-[6px] border border-line bg-white px-3 text-[13px] outline-none focus:border-primary"
                    />
                  </label>

                  <label class="grid gap-1">
                    <span class="text-[12px] font-medium text-textMuted">System Prompt</span>
                    <textarea
                      v-model.trim="skill.systemPrompt"
                      class="h-[88px] resize-none rounded-[6px] border border-line bg-white px-3 py-2 text-[12px] leading-[1.5] outline-none focus:border-primary"
                    />
                  </label>

                  <label class="grid gap-1">
                    <span class="text-[12px] font-medium text-textMuted">任务 Prompt</span>
                    <textarea
                      v-model.trim="skill.taskPrompt"
                      class="h-[72px] resize-none rounded-[6px] border border-line bg-white px-3 py-2 text-[12px] leading-[1.5] outline-none focus:border-primary"
                    />
                  </label>

                  <label class="grid gap-1">
                    <span class="text-[12px] font-medium text-textMuted">输出格式</span>
                    <textarea
                      v-model.trim="skill.outputFormat"
                      class="h-[72px] resize-none rounded-[6px] border border-line bg-white px-3 py-2 text-[12px] leading-[1.5] outline-none focus:border-primary"
                    />
                  </label>

                  <label class="grid gap-1">
                    <span class="flex items-center justify-between text-[12px] font-medium text-textMuted">
                      <span>温度</span>
                      <span>{{ skill.temperature }}</span>
                    </span>
                    <div class="flex items-center gap-2">
                      <input
                        v-model.number="skill.temperature"
                        class="h-2 min-w-0 flex-1 accent-primary"
                        type="range"
                        min="0"
                        max="1"
                        step="0.05"
                      />
                      <input
                        v-model.number="skill.temperature"
                        class="h-9 w-20 rounded-[6px] border border-line bg-white px-2 text-[13px] outline-none focus:border-primary"
                        type="number"
                        min="0"
                        max="1"
                        step="0.05"
                      />
                    </div>
                  </label>

                  <div class="flex justify-end gap-2 pt-1">
                    <button
                      class="h-8 rounded-[6px] border border-line bg-white px-3 text-[12px] font-medium text-textMain hover:bg-panel"
                      @click="resetAiSkill(skill.id)"
                    >
                      重置此技能
                    </button>
                    <button
                      class="h-8 rounded-[6px] bg-primary px-3 text-[12px] font-medium text-white hover:bg-blue-600"
                      @click="saveAiSkillSettings"
                    >
                      保存技能
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section class="mb-5 flex items-center justify-between rounded-[6px] bg-panel px-3 py-3">
          <div>
            <h2 class="text-[13px] font-semibold">开机自启</h2>
            <p class="text-[12px] text-textMuted">启动后静默驻留托盘</p>
          </div>
          <button
            class="flex h-7 w-12 items-center rounded-full p-0.5 transition-colors"
            :class="config.autostart ? 'bg-primary' : 'bg-line'"
            title="开机自启"
            @click="toggleAutostart"
          >
            <span
              class="h-6 w-6 rounded-full bg-white shadow-soft transition-transform"
              :class="config.autostart ? 'translate-x-5' : 'translate-x-0'"
            />
          </button>
        </section>

        <div v-if="errorText" class="rounded-[6px] border border-red-100 bg-red-50 px-3 py-2 text-[12px] text-loss">
          {{ errorText }}
        </div>
      </div>

      <footer class="shrink-0 border-t border-line bg-white px-4 py-3">
        <div class="grid gap-2">
          <button
            class="h-10 rounded-[6px] border border-line bg-white text-[13px] font-medium text-loss hover:bg-red-50"
            @click="resetConfig"
          >
            重置配置
          </button>
          <button
            class="h-10 rounded-[6px] bg-primary text-[13px] font-semibold text-white hover:bg-blue-600"
            @click="saveAllSettings"
          >
            保存设置
          </button>
        </div>
      </footer>
    </section>

    <transition enter-active-class="transition duration-200 ease-out" enter-from-class="translate-x-full opacity-0" enter-to-class="translate-x-0 opacity-100" leave-active-class="transition duration-150 ease-in" leave-from-class="translate-x-0 opacity-100" leave-to-class="translate-x-full opacity-0">
      <aside v-if="isAiAssistantOpen" class="absolute inset-0 z-40 flex h-full w-full flex-col border-l border-line bg-white shadow-popup">
        <header class="flex h-[56px] shrink-0 items-center justify-between border-b border-line px-5">
          <div class="flex min-w-0 items-center gap-2">
            <span class="grid h-8 w-8 shrink-0 place-items-center rounded-full bg-blue-100 text-primary">
              <Bot class="h-4 w-4" />
            </span>
            <select
              v-model="selectedAiModelId"
              class="h-8 min-w-0 max-w-[220px] rounded-[6px] border border-transparent bg-white px-1 text-[14px] font-semibold text-textMain outline-none hover:border-line focus:border-primary"
              title="选择AI模型"
            >
              <option v-if="!config.aiModels.length" value="">未配置模型</option>
              <option v-for="model in config.aiModels" :key="model.id" :value="model.id">
                {{ model.name }}
              </option>
            </select>
          </div>
          <div class="flex items-center gap-1">
            <button
              class="grid h-8 w-8 place-items-center rounded-[6px] text-textMuted hover:bg-red-50 hover:text-loss disabled:cursor-not-allowed disabled:opacity-40"
              title="删除历史记录"
              :disabled="!aiMessages.length"
              @click="clearAiHistory"
            >
              <Trash2 class="h-4 w-4" />
            </button>
            <button class="grid h-8 w-8 place-items-center rounded-[6px] text-textMuted hover:bg-panel hover:text-textMain" title="AI模型设置" @click="openAiSettings">
              <Settings class="h-4 w-4" />
            </button>
            <button class="grid h-8 w-8 place-items-center rounded-[6px] text-textMuted hover:bg-panel hover:text-textMain" title="关闭AI助手" @click="closeAiAssistant">
              <X class="h-4 w-4" />
            </button>
          </div>
        </header>

        <div ref="aiConversationRef" class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          <div v-if="!aiMessages.length" class="grid h-full place-items-center text-center text-[12px] text-textMuted">
            当前暂无对话
          </div>
          <div v-else class="space-y-3">
            <article
              v-for="message in aiMessages"
              :key="message.id"
              class="flex"
              :class="message.role === 'user' ? 'justify-end' : 'justify-start'"
            >
              <div
                class="relative max-w-[86%] rounded-[6px] px-3 py-2 text-[13px] leading-[1.6]"
                :class="message.role === 'user' ? 'bg-primary text-white' : 'bg-panel text-textMain'"
              >
                <p v-if="message.role === 'user'" class="whitespace-pre-wrap">{{ message.content }}</p>
                <div v-else-if="message.status === 'thinking'" class="inline-flex items-center gap-1.5 text-textMuted">
                  <span>思考中</span>
                  <span class="ai-thinking-dot" />
                  <span class="ai-thinking-dot animation-delay-150" />
                  <span class="ai-thinking-dot animation-delay-300" />
                </div>
                <div v-else-if="message.status === 'error'" class="text-loss">
                  {{ message.errorMessage || '当前回答生成失败，请稍后重试' }}
                </div>
                <div v-else class="ai-message-markdown">
                  <div v-html="renderAiMessageMarkdown(message)" />
                  <span v-if="message.status === 'streaming'" class="ai-streaming-caret" />
                </div>
                <button
                  v-if="message.role === 'assistant'"
                  class="absolute bottom-1 right-1 grid h-6 w-6 place-items-center rounded-[6px] text-textMuted hover:bg-white hover:text-textMain"
                  title="复制回复"
                  @click="copyAssistantMessage(getAiMessageDisplayContent(message))"
                >
                  <Copy class="h-3.5 w-3.5" />
                </button>
              </div>
            </article>
          </div>
        </div>

        <section class="shrink-0 border-t border-line px-5 py-3">
          <div class="mb-2 text-[12px] font-semibold text-textMuted">快捷技能</div>
          <div class="flex flex-wrap gap-2">
            <button
              v-for="skill in config.aiSkills"
              :key="skill.id"
              class="inline-flex h-8 items-center gap-1.5 rounded-full border px-3 text-[12px] font-medium transition disabled:cursor-not-allowed disabled:opacity-50"
              :class="isAiSkillSelected(skill.id) ? 'border-primary bg-blue-50 text-primary' : 'border-transparent bg-panel text-textMain hover:bg-blue-50 hover:text-primary'"
              :aria-pressed="isAiSkillSelected(skill.id)"
              :disabled="aiSending"
              @click="toggleAiSkill(skill.id)"
            >
              <component :is="getAiSkillIcon(skill.id)" class="h-4 w-4" />
              <span>{{ skill.label }}</span>
            </button>
          </div>
        </section>

        <footer class="shrink-0 px-5 pb-4">
          <div class="relative">
            <textarea
              v-model="aiQuestion"
              class="h-[98px] w-full resize-none rounded-[6px] border border-line bg-white px-3 py-3 pr-14 text-[13px] outline-none placeholder:text-textMuted focus:border-primary"
              :placeholder="selectedAiSkillLabels.length ? `补充要求（已选：${selectedAiSkillLabels.join('、')}）` : '请输入问题，AI将基于当前周报数据回答...'"
              @keydown.enter.exact.prevent="sendAiQuestion"
            />
            <button
              class="absolute bottom-3 right-3 grid h-9 w-9 place-items-center rounded-[6px] bg-primary text-white hover:bg-blue-600 disabled:cursor-not-allowed disabled:bg-blue-200"
              :disabled="aiSending || !canSendAiMessage"
              title="发送"
              @click="sendAiQuestion"
            >
              <Loader2 v-if="aiSending" class="h-4 w-4 animate-spin" />
              <Send v-else class="h-4 w-4" />
            </button>
          </div>
          <div class="mt-2 flex items-center gap-1.5 text-[12px] text-textMuted">
            <span class="grid h-4 w-4 place-items-center rounded-full border border-line text-[10px]">i</span>
            <span>仅使用统计数据，不读取或上传源代码</span>
          </div>
        </footer>
      </aside>
    </transition>

    <transition enter-active-class="transition duration-200" enter-from-class="opacity-0 scale-95" enter-to-class="opacity-100 scale-100" leave-active-class="transition duration-150" leave-from-class="opacity-100 scale-100" leave-to-class="opacity-0 scale-95">
      <div v-if="toast" class="absolute bottom-16 left-1/2 inline-flex -translate-x-1/2 items-center gap-2 rounded-[6px] bg-textMain px-3 py-2 text-[12px] text-white shadow-popup">
        <Check class="h-4 w-4 text-gain" />
        <span>{{ toast }}</span>
      </div>
    </transition>
  </main>
</template>

<style>
.ai-thinking-dot {
  width: 4px;
  height: 4px;
  border-radius: 999px;
  background: #6b7280;
  animation: ai-thinking-pulse 1s infinite ease-in-out;
}

.animation-delay-150 {
  animation-delay: 0.15s;
}

.animation-delay-300 {
  animation-delay: 0.3s;
}

.ai-streaming-caret {
  display: inline-block;
  width: 7px;
  height: 14px;
  margin-left: 2px;
  vertical-align: -2px;
  background: #3b82f6;
  animation: ai-caret-blink 0.9s infinite;
}

.ai-message-markdown .ai-md-root {
  padding-right: 18px;
  color: #1f2937;
}

.ai-message-markdown .ai-md-root > *:first-child {
  margin-top: 0;
}

.ai-message-markdown .ai-md-root > *:last-child {
  margin-bottom: 0;
}

.ai-message-markdown h1,
.ai-message-markdown h2,
.ai-message-markdown h3,
.ai-message-markdown h4 {
  margin: 10px 0 6px;
  font-weight: 700;
  line-height: 1.35;
  color: #111827;
}

.ai-message-markdown h1 {
  font-size: 18px;
}

.ai-message-markdown h2 {
  font-size: 16px;
}

.ai-message-markdown h3 {
  font-size: 14px;
}

.ai-message-markdown h4 {
  font-size: 13px;
}

.ai-message-markdown p {
  margin: 6px 0;
}

.ai-message-markdown ul,
.ai-message-markdown ol {
  margin: 6px 0;
  padding-left: 18px;
}

.ai-message-markdown li {
  margin: 3px 0;
}

.ai-message-markdown strong {
  font-weight: 700;
  color: #111827;
}

.ai-message-markdown blockquote {
  margin: 8px 0;
  padding: 8px 10px;
  color: #4b5563;
  background: #eef4ff;
  border-left: 3px solid #3b82f6;
  border-radius: 0 6px 6px 0;
}

.ai-message-markdown code {
  padding: 1px 4px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
  font-size: 12px;
  color: #1f2937;
  background: #eef2f7;
  border-radius: 4px;
}

.ai-md-code-block {
  margin: 10px 0;
  overflow: hidden;
  border: 1px solid #e5e7eb;
  border-radius: 6px;
  background: #f8fafc;
}

.ai-md-code-label {
  height: 28px;
  padding: 0 10px;
  font-size: 11px;
  font-weight: 600;
  line-height: 28px;
  color: #6b7280;
  background: #f3f4f6;
  border-bottom: 1px solid #e5e7eb;
}

.ai-md-code-block pre {
  margin: 0;
  padding: 10px;
  overflow-x: auto;
}

.ai-md-code-block pre code {
  padding: 0;
  background: transparent;
  border-radius: 0;
}

.ai-md-table-wrap {
  max-width: 100%;
  margin: 10px 0;
  overflow-x: auto;
  border: 1px solid #e5e7eb;
  border-radius: 6px;
}

.ai-md-table-wrap table {
  width: 100%;
  min-width: 900px;
  border-collapse: collapse;
  font-size: 12px;
  table-layout: auto;
}

.ai-md-table-wrap th,
.ai-md-table-wrap td {
  padding: 8px 10px;
  text-align: left;
  border-bottom: 1px solid #e5e7eb;
  border-right: 1px solid #e5e7eb;
  vertical-align: top;
  white-space: normal;
  word-break: normal;
  overflow-wrap: break-word;
  line-height: 1.55;
}

.ai-md-table-wrap th {
  font-weight: 700;
  color: #374151;
  background: #f9fafb;
  white-space: nowrap;
}

.ai-md-table-wrap tr:last-child td {
  border-bottom: 0;
}

.ai-md-table-wrap th:last-child,
.ai-md-table-wrap td:last-child {
  border-right: 0;
}

@keyframes ai-thinking-pulse {
  0%,
  80%,
  100% {
    opacity: 0.35;
    transform: translateY(0);
  }
  40% {
    opacity: 1;
    transform: translateY(-2px);
  }
}

@keyframes ai-caret-blink {
  0%,
  45% {
    opacity: 1;
  }
  46%,
  100% {
    opacity: 0;
  }
}
</style>
