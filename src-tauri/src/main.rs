#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewWindow, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use wait_timeout::ChildExt;

const MAX_SCAN_DEPTH: usize = 6;
const ROOT_SCAN_TIMEOUT: Duration = Duration::from_secs(10);
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
const COMMIT_MARKER: &str = "__GIT_WEEKLY_COMMIT__";
const MERGE_MARKER: &str = "__GIT_WEEKLY_MERGE__";
const DEFAULT_TARGET_BRANCH: &str = "main";
const MAX_DIFF_SUMMARY_REQUESTS: usize = 60;
const MAX_DIFF_SUMMARY_CHARS: usize = 1600;
const MAX_DIFF_SUMMARY_LINES: usize = 40;
const MAX_DIFF_SUMMARY_LINE_CHARS: usize = 180;
const MAX_DIFF_DETAIL_FILES: usize = 24;
const MAX_DIFF_DETAIL_LINES: usize = 240;
const MAX_DIFF_DETAIL_LINE_CHARS: usize = 220;

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "vendor",
    ".idea",
    ".vscode",
    "dist",
    "build",
    ".next",
    ".cache",
    "Pods",
    "DerivedData",
];

#[derive(Default)]
struct AppState {
    hide_suspended: AtomicBool,
    positioned_once: AtomicBool,
    last_window_interaction_at: Mutex<Option<Instant>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct AppConfig {
    scan_dirs: Vec<String>,
    emails: Vec<String>,
    autostart: bool,
    target_branch: String,
    date_range_start: String,
    date_range_end: String,
    ai_models: Vec<AiModelConfig>,
    default_ai_model_id: String,
    ai_skills: Vec<AiSkillConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct AiModelConfig {
    id: String,
    name: String,
    provider: String,
    model: String,
    base_url: String,
    api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct AiSkillConfig {
    id: String,
    label: String,
    description: String,
    system_prompt: String,
    task_prompt: String,
    output_format: String,
    context_policy: String,
    temperature: f32,
}

impl Default for AiModelConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            provider: String::new(),
            model: String::new(),
            base_url: String::new(),
            api_key: String::new(),
        }
    }
}

impl Default for AiSkillConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            description: String::new(),
            system_prompt: String::new(),
            task_prompt: String::new(),
            output_format: String::new(),
            context_policy: "optional-git".to_string(),
            temperature: 0.3,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            scan_dirs: Vec::new(),
            emails: Vec::new(),
            autostart: false,
            target_branch: DEFAULT_TARGET_BRANCH.to_string(),
            date_range_start: String::new(),
            date_range_end: String::new(),
            ai_models: default_ai_models(),
            default_ai_model_id: "openai-gpt-4o".to_string(),
            ai_skills: default_ai_skills(),
        }
    }
}

fn default_ai_models() -> Vec<AiModelConfig> {
    vec![
        AiModelConfig {
            id: "openai-gpt-4o".to_string(),
            name: "OpenAI GPT-4o".to_string(),
            provider: "OpenAI".to_string(),
            model: "gpt-4o".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
        },
        AiModelConfig {
            id: "qwen-3-5".to_string(),
            name: "通义千问 3.5".to_string(),
            provider: "DashScope".to_string(),
            model: "qwen-plus".to_string(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            api_key: String::new(),
        },
        AiModelConfig {
            id: "deepseek-chat".to_string(),
            name: "DeepSeek Chat".to_string(),
            provider: "DeepSeek".to_string(),
            model: "deepseek-chat".to_string(),
            base_url: "https://api.deepseek.com".to_string(),
            api_key: String::new(),
        },
    ]
}

fn default_ai_skills() -> Vec<AiSkillConfig> {
    vec![
        AiSkillConfig {
            id: "weekly".to_string(),
            label: "标准周报".to_string(),
            description: "把 Git 记录整理成可直接发送的周报。".to_string(),
            system_prompt: "你是专业的研发周报助手，擅长把 Git 提交、合并记录、文件级真实改动和用户补充内容整理成清晰、可信的中文工作周报。".to_string(),
            task_prompt: "请生成一份标准研发周报。若 commit 文案质量较低，请以真实改动依据为主进行总结。".to_string(),
            output_format: "输出结构：本周概览、项目进展、关键改动、风险与阻塞、下周计划。".to_string(),
            context_policy: "required-git".to_string(),
            temperature: 0.3,
        },
        AiSkillConfig {
            id: "month".to_string(),
            label: "月度总结".to_string(),
            description: "总结一个月的研发进展、产出和风险。".to_string(),
            system_prompt: "你是研发月报助手，擅长结合 Git 统计、合并记录和文件级真实改动，把阶段性工作整理成管理者可快速阅读的月度总结。".to_string(),
            task_prompt: "请生成一份月度工作总结。若 commit 文案质量较低，请以真实改动依据为主进行总结。".to_string(),
            output_format: "输出结构：月度概览、主要成果、项目进展、风险问题、下月重点。".to_string(),
            context_policy: "required-git".to_string(),
            temperature: 0.3,
        },
        AiSkillConfig {
            id: "workload".to_string(),
            label: "工作量评估".to_string(),
            description: "按新需求拆解功能、复杂度、优先级、风险和验收注意点。".to_string(),
            system_prompt: "你是研发需求评估助手，擅长基于 Git 统计、文件级真实改动、diff 摘要和用户描述，把代码改动反推成新需求功能块，并评估复杂度、优先级、风险和验收注意点。".to_string(),
            task_prompt: "请按“新需求评估”的视角分析当前工作量。若 commit 文案质量较低，请以真实改动依据为主，把相近提交合并成可评审的功能项。".to_string(),
            output_format: "输出结构：总体结论、功能排期评估表、高中低风险、验收注意点。功能排期评估表列为：功能项、需求类型、复杂度、优先级、预估耗时、前置依赖、风险等级、注意点；每个功能点都必须填写预估耗时，可用 0.5天、1天、2-3天、3-5天 这类粗粒度表达，不要输出具体日期。".to_string(),
            context_policy: "optional-git".to_string(),
            temperature: 0.25,
        },
        AiSkillConfig {
            id: "review".to_string(),
            label: "改动复盘".to_string(),
            description: "复盘代码改动目的、影响范围和风险。".to_string(),
            system_prompt: "你是代码改动复盘助手，关注变更目的、影响范围、潜在风险和后续验证建议。判断依据以文件级真实改动为主，commit 文案仅作参考。".to_string(),
            task_prompt: "请对当前改动进行复盘。不要按 commit 文案硬套分类，请根据真实改动依据推断实际影响。".to_string(),
            output_format: "输出结构：改动摘要、影响范围、潜在风险、验证建议、后续动作。".to_string(),
            context_policy: "required-git".to_string(),
            temperature: 0.25,
        },
        AiSkillConfig {
            id: "insight".to_string(),
            label: "真实功能总结".to_string(),
            description: "根据文件级真实改动推断实际实现的功能。".to_string(),
            system_prompt: "你是代码变更理解助手，擅长根据 Git 文件级改动信息推断实际完成的功能、影响范围和可信度。不要依赖低质量 commit 文案，不要用正则分类，也不要编造未提供的源代码细节。".to_string(),
            task_prompt: "请基于真实改动依据，总结当前筛选范围内每个项目实际做了什么功能或改动。".to_string(),
            output_format: "输出结构：整体结论、按项目功能总结、按提交关键改动、低可信或需补充上下文的部分、建议验证点。".to_string(),
            context_policy: "required-git".to_string(),
            temperature: 0.2,
        },
        AiSkillConfig {
            id: "daily".to_string(),
            label: "精简日报".to_string(),
            description: "生成短小、直接的日报内容。".to_string(),
            system_prompt: "你是日报写作助手，擅长结合文件级真实改动，把工作内容压缩成简洁、自然、可直接发送的中文日报。".to_string(),
            task_prompt: "请生成一份精简日报。若 commit 文案质量较低，请以真实改动依据为主进行总结。".to_string(),
            output_format: "输出结构：今日完成、进行中、问题风险、明日计划。每部分尽量精简。".to_string(),
            context_policy: "optional-git".to_string(),
            temperature: 0.35,
        },
    ]
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    total_commits: u64,
    total_additions: u64,
    total_deletions: u64,
    repo_count: u64,
    merged_branch_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommitFileChange {
    path: String,
    additions: Option<u64>,
    deletions: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommitRecord {
    repo_name: String,
    repo_path: String,
    hash: String,
    author_name: String,
    author_email: String,
    author_date: String,
    subject: String,
    additions: Option<u64>,
    deletions: Option<u64>,
    files: Vec<CommitFileChange>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitDiffRequest {
    repo_path: String,
    hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommitDiffSummary {
    repo_path: String,
    hash: String,
    excerpt: String,
    files: Vec<CommitDiffFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommitDiffFile {
    path: String,
    lines: Vec<CommitDiffLine>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommitDiffLine {
    kind: String,
    content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MergedBranchRecord {
    repo_name: String,
    repo_path: String,
    target_branch: String,
    target_ref: String,
    source_branch: Option<String>,
    hash: String,
    committer_name: String,
    committer_email: String,
    merged_at: String,
    subject: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanIssue {
    path: Option<String>,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WeeklyReport {
    summary: Summary,
    commits: Vec<CommitRecord>,
    merged_branches: Vec<MergedBranchRecord>,
    issues: Vec<ScanIssue>,
    scanned_at: String,
    week_start: String,
    week_end: String,
    target_branch: String,
}

#[derive(Debug, Clone)]
struct RawCommit {
    hash: String,
    author_name: String,
    author_email: String,
    author_date: String,
    subject: String,
    additions: u64,
    deletions: u64,
    files: Vec<CommitFileChange>,
}

#[derive(Debug, Clone)]
struct RawMerge {
    hash: String,
    committer_name: String,
    committer_email: String,
    merged_at: String,
    subject: String,
}

#[derive(Debug)]
enum GitCommandError {
    Missing,
    Timeout,
    Exit(String),
    Io(String),
}

impl std::fmt::Display for GitCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitCommandError::Missing => write!(formatter, "未检测到本地 Git 环境，请先安装 Git"),
            GitCommandError::Timeout => write!(formatter, "Git 命令执行超时"),
            GitCommandError::Exit(message) => write!(formatter, "Git 命令执行失败：{message}"),
            GitCommandError::Io(message) => write!(formatter, "Git 命令执行失败：{message}"),
        }
    }
}

#[tauri::command]
fn load_config(app: AppHandle) -> Result<AppConfig, String> {
    let path = config_path(&app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let content = fs::read_to_string(&path).map_err(|error| format!("读取配置失败：{error}"))?;
    serde_json::from_str(&content).map_err(|error| format!("解析配置失败：{error}"))
}

#[tauri::command]
fn save_config(app: AppHandle, config: AppConfig) -> Result<(), String> {
    let path = config_path(&app)?;
    let content = serde_json::to_string_pretty(&config)
        .map_err(|error| format!("序列化配置失败：{error}"))?;
    fs::write(path, content).map_err(|error| format!("保存配置失败：{error}"))
}

#[tauri::command]
fn reset_config(app: AppHandle) -> Result<(), String> {
    let path = config_path(&app)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("重置配置失败：{error}"))?;
    }
    Ok(())
}

#[tauri::command]
fn set_auto_hide_suspended(state: State<'_, AppState>, suspended: bool) {
    state.hide_suspended.store(suspended, Ordering::SeqCst);
}

#[tauri::command]
async fn get_email_candidates(scan_dirs: Vec<String>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || collect_email_candidates(&scan_dirs))
        .await
        .map_err(|error| format!("读取 Git 邮箱失败：{error}"))?
}

#[tauri::command]
async fn scan_weekly_report(config: AppConfig) -> Result<WeeklyReport, String> {
    tauri::async_runtime::spawn_blocking(move || scan_weekly_report_sync(config))
        .await
        .map_err(|error| format!("扫描任务失败：{error}"))?
}

#[tauri::command]
async fn get_commit_diff_summaries(
    requests: Vec<CommitDiffRequest>,
) -> Result<Vec<CommitDiffSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || collect_commit_diff_summaries(requests))
        .await
        .map_err(|error| format!("读取提交改动失败：{error}"))?
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState::default())
        .setup(|app| {
            setup_window_events(app.handle().clone());
            setup_tray(app)?;
            show_main_window(app.handle());
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "refresh" => {
                show_main_window(app);
                let _ = app.emit("refresh-requested", ());
            }
            "settings" => {
                show_main_window(app);
                let _ = app.emit("navigate-settings", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|app, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(app);
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            reset_config,
            set_auto_hide_suspended,
            get_email_candidates,
            scan_weekly_report,
            get_commit_diff_summaries
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Git weekly tray app");
}

fn setup_tray(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "刷新", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &refresh, &settings, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .tooltip("GitSage")
        .icon(tray_icon())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .build(app)?;

    Ok(())
}

fn setup_window_events(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let app_for_event = app.clone();
        window.on_window_event(move |event| match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                remember_window_interaction(&app_for_event);
            }
            WindowEvent::Focused(false) => {
                let app = app_for_event.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    std::thread::sleep(Duration::from_millis(240));
                    let state = app.state::<AppState>();
                    if state.hide_suspended.load(Ordering::SeqCst)
                        || recently_interacted_with_window(&state)
                    {
                        return;
                    }
                    let Some(window) = app.get_webview_window("main") else {
                        return;
                    };
                    if window.is_focused().unwrap_or(false) {
                        return;
                    }
                    let _ = window.hide();
                });
            }
            _ => {}
        });
    }
}

fn remember_window_interaction(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Ok(mut last_interaction) = state.last_window_interaction_at.lock() {
        *last_interaction = Some(Instant::now());
    };
}

fn recently_interacted_with_window(state: &AppState) -> bool {
    state
        .last_window_interaction_at
        .lock()
        .ok()
        .and_then(|last_interaction| *last_interaction)
        .is_some_and(|last_interaction| last_interaction.elapsed() < Duration::from_millis(900))
}

fn toggle_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            show_window(app, &window);
        }
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        show_window(app, &window);
    }
}

fn show_window(app: &AppHandle, window: &WebviewWindow) {
    let state = app.state::<AppState>();
    if !state.positioned_once.swap(true, Ordering::SeqCst) {
        position_window(window);
    }
    let _ = window.show();
    let _ = window.set_focus();
}

fn position_window(window: &WebviewWindow) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        return;
    };

    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let window_size = window.outer_size().ok();
    let width = window_size.map(|size| size.width as i32).unwrap_or(480);
    let height = window_size.map(|size| size.height as i32).unwrap_or(620);
    let padding = 12;

    let x = monitor_pos.x + monitor_size.width as i32 - width - padding;
    let y = if cfg!(target_os = "macos") {
        monitor_pos.y + 28
    } else {
        monitor_pos.y + monitor_size.height as i32 - height - 48
    };

    let min_x = monitor_pos.x + padding;
    let max_x = monitor_pos.x + monitor_size.width as i32 - width - padding;
    let min_y = monitor_pos.y + padding;
    let max_y = monitor_pos.y + monitor_size.height as i32 - height - padding;

    let _ = window.set_position(PhysicalPosition::new(
        x.clamp(min_x, max_x),
        y.clamp(min_y, max_y),
    ));
}

fn tray_icon() -> Image<'static> {
    let size = 32;
    let mut rgba = Vec::with_capacity(size * size * 4);

    for y in 0..size {
        for x in 0..size {
            let dx = x as f64 - 15.5;
            let dy = y as f64 - 15.5;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance <= 13.5 {
                rgba.extend_from_slice(&[59, 130, 246, 255]);
            } else if distance <= 15.0 {
                rgba.extend_from_slice(&[59, 130, 246, 120]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    Image::new_owned(rgba, size as u32, size as u32)
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法获取配置目录：{error}"))?;
    fs::create_dir_all(&dir).map_err(|error| format!("无法创建配置目录：{error}"))?;
    Ok(dir.join("config.json"))
}

fn collect_email_candidates(scan_dirs: &[String]) -> Result<Vec<String>, String> {
    let mut emails = HashSet::new();

    if let Ok(output) = run_git(
        &["config", "--global", "user.email"],
        Duration::from_secs(2),
    ) {
        add_email_candidate(&mut emails, output.trim());
    }

    let mut repos = HashSet::new();
    let mut issues = Vec::new();
    for dir in scan_dirs {
        let root = PathBuf::from(dir);
        if root.exists() && root.is_dir() {
            collect_repos(
                &root,
                &root,
                0,
                Instant::now() + Duration::from_secs(5),
                &mut repos,
                &mut issues,
            );
        }
    }

    for repo in repos {
        let args = vec![
            "-C".to_string(),
            repo.to_string_lossy().into_owned(),
            "config".to_string(),
            "user.email".to_string(),
        ];
        if let Ok(output) = run_git_owned(&args, Duration::from_secs(1)) {
            add_email_candidate(&mut emails, output.trim());
        }
    }

    let mut result: Vec<String> = emails.into_iter().collect();
    result.sort();
    Ok(result)
}

fn add_email_candidate(emails: &mut HashSet<String>, value: &str) {
    let email = value.trim().to_lowercase();
    if email.contains('@') {
        emails.insert(email);
    }
}

fn collect_commit_diff_summaries(
    requests: Vec<CommitDiffRequest>,
) -> Result<Vec<CommitDiffSummary>, String> {
    let mut summaries = Vec::new();
    let mut seen = HashSet::new();

    for request in requests.into_iter().take(MAX_DIFF_SUMMARY_REQUESTS) {
        let repo_path = request.repo_path.trim().to_string();
        let hash = request.hash.trim().to_string();
        if repo_path.is_empty() || !is_safe_commit_hash(&hash) {
            continue;
        }

        let key = format!("{repo_path}\u{1f}{hash}");
        if !seen.insert(key) {
            continue;
        }

        let repo = PathBuf::from(&repo_path);
        if !repo.join(".git").exists() {
            continue;
        }

        let args = vec![
            "-C".to_string(),
            repo_path.clone(),
            "show".to_string(),
            "--format=".to_string(),
            "--no-ext-diff".to_string(),
            "--no-color".to_string(),
            "--unified=0".to_string(),
            hash.clone(),
        ];

        if let Ok(output) = run_git_owned(&args, GIT_COMMAND_TIMEOUT) {
            summaries.push(CommitDiffSummary {
                repo_path,
                hash,
                excerpt: build_diff_excerpt(&output),
                files: build_diff_files(&output),
            });
        }
    }

    Ok(summaries)
}

fn is_safe_commit_hash(value: &str) -> bool {
    let length = value.len();
    (7..=64).contains(&length) && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn build_diff_excerpt(output: &str) -> String {
    let mut lines = Vec::new();

    for line in output.lines() {
        if lines.len() >= MAX_DIFF_SUMMARY_LINES {
            break;
        }

        if let Some(path) = parse_diff_file_path(line) {
            push_diff_excerpt_line(&mut lines, format!("文件：{path}"));
            continue;
        }

        if line.starts_with("@@") {
            push_diff_excerpt_line(&mut lines, format!("位置：{}", line.trim()));
            continue;
        }

        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }

        if let Some(content) = line.strip_prefix('+') {
            let content = content.trim();
            if !content.is_empty() {
                push_diff_excerpt_line(&mut lines, format!("新增：{content}"));
            }
            continue;
        }

        if let Some(content) = line.strip_prefix('-') {
            let content = content.trim();
            if !content.is_empty() {
                push_diff_excerpt_line(&mut lines, format!("删除：{content}"));
            }
        }
    }

    truncate_chars(&lines.join("\n"), MAX_DIFF_SUMMARY_CHARS)
}

fn build_diff_files(output: &str) -> Vec<CommitDiffFile> {
    let mut files = Vec::new();
    let mut current_file: Option<CommitDiffFile> = None;
    let mut total_lines = 0;

    for line in output.lines() {
        if let Some(path) = parse_diff_file_path(line) {
            if let Some(file) = current_file.take() {
                if !file.lines.is_empty() {
                    files.push(file);
                }
            }

            if files.len() >= MAX_DIFF_DETAIL_FILES || total_lines >= MAX_DIFF_DETAIL_LINES {
                break;
            }

            current_file = Some(CommitDiffFile {
                path,
                lines: Vec::new(),
            });
            continue;
        }

        if total_lines >= MAX_DIFF_DETAIL_LINES {
            break;
        }

        let Some(file) = current_file.as_mut() else {
            continue;
        };

        let diff_line = if line.starts_with("@@") {
            Some(CommitDiffLine {
                kind: "hunk".to_string(),
                content: truncate_chars(line.trim(), MAX_DIFF_DETAIL_LINE_CHARS),
            })
        } else if line.starts_with("+++") || line.starts_with("---") {
            None
        } else if let Some(content) = line.strip_prefix('+') {
            Some(CommitDiffLine {
                kind: "add".to_string(),
                content: truncate_chars(content, MAX_DIFF_DETAIL_LINE_CHARS),
            })
        } else {
            line.strip_prefix('-').map(|content| CommitDiffLine {
                kind: "delete".to_string(),
                content: truncate_chars(content, MAX_DIFF_DETAIL_LINE_CHARS),
            })
        };

        if let Some(diff_line) = diff_line {
            file.lines.push(diff_line);
            total_lines += 1;
        }
    }

    if let Some(file) = current_file.take() {
        if !file.lines.is_empty() && files.len() < MAX_DIFF_DETAIL_FILES {
            files.push(file);
        }
    }

    files
}

fn parse_diff_file_path(line: &str) -> Option<String> {
    if !line.starts_with("diff --git ") {
        return None;
    }

    let (_, path) = line.rsplit_once(" b/")?;
    Some(normalize_numstat_path(path.trim().trim_matches('"')))
}

fn push_diff_excerpt_line(lines: &mut Vec<String>, line: String) {
    if lines.len() >= MAX_DIFF_SUMMARY_LINES {
        return;
    }
    lines.push(truncate_chars(&line, MAX_DIFF_SUMMARY_LINE_CHARS));
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut result = String::new();
    for (index, character) in value.chars().enumerate() {
        if index >= max_chars {
            result.push('…');
            break;
        }
        result.push(character);
    }
    result
}

fn scan_weekly_report_sync(config: AppConfig) -> Result<WeeklyReport, String> {
    let now = Local::now();
    let (range_start, range_end) = resolve_scan_range(&config, now)?;
    let range_start_string = range_start.to_rfc3339();
    let range_end_string = range_end.to_rfc3339();
    let scanned_at = now.to_rfc3339();
    let target_branch = normalize_target_branch(&config.target_branch);
    let mut issues = Vec::new();
    let mut commits = Vec::new();
    let mut merged_branches = Vec::new();

    if let Err(error) = run_git(&["--version"], Duration::from_secs(2)) {
        issues.push(ScanIssue {
            path: None,
            message: match error {
                GitCommandError::Missing => "未检测到本地 Git 环境，请先安装 Git".to_string(),
                _ => error.to_string(),
            },
        });
        return Ok(build_report(
            commits,
            merged_branches,
            issues,
            scanned_at,
            range_start_string,
            range_end_string,
            target_branch,
        ));
    }

    let emails: HashSet<String> = config
        .emails
        .iter()
        .map(|email| email.trim().to_lowercase())
        .filter(|email| email.contains('@'))
        .collect();

    if emails.is_empty() {
        issues.push(ScanIssue {
            path: None,
            message: "请配置 Git 个人邮箱".to_string(),
        });
        return Ok(build_report(
            commits,
            merged_branches,
            issues,
            scanned_at,
            range_start_string,
            range_end_string,
            target_branch,
        ));
    }

    let repos = scan_roots(&config.scan_dirs, &mut issues);

    for repo in repos {
        match scan_repo(&repo, &emails, &range_start_string, &range_end_string) {
            Ok(mut repo_commits) => commits.append(&mut repo_commits),
            Err(error) => issues.push(ScanIssue {
                path: Some(repo.to_string_lossy().into_owned()),
                message: error.to_string(),
            }),
        }

        match scan_repo_merges(
            &repo,
            &target_branch,
            &range_start_string,
            &range_end_string,
        ) {
            Ok(mut repo_merges) => merged_branches.append(&mut repo_merges),
            Err(error) => issues.push(ScanIssue {
                path: Some(repo.to_string_lossy().into_owned()),
                message: error.to_string(),
            }),
        }
    }

    commits.sort_by(|left, right| right.author_date.cmp(&left.author_date));
    merged_branches.sort_by(|left, right| right.merged_at.cmp(&left.merged_at));

    Ok(build_report(
        commits,
        merged_branches,
        issues,
        scanned_at,
        range_start_string,
        range_end_string,
        target_branch,
    ))
}

fn resolve_scan_range(
    config: &AppConfig,
    now: chrono::DateTime<Local>,
) -> Result<(chrono::DateTime<Local>, chrono::DateTime<Local>), String> {
    let default_start = start_of_week(now)?;
    let start = parse_config_date(&config.date_range_start, false)?.unwrap_or(default_start);
    let end = parse_config_date(&config.date_range_end, true)?.unwrap_or(now);

    if start > end {
        return Err("开始日期不能晚于结束日期".to_string());
    }

    Ok((start, end))
}

fn parse_config_date(
    value: &str,
    end_of_day: bool,
) -> Result<Option<chrono::DateTime<Local>>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let date = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .map_err(|_| format!("日期格式无效：{trimmed}"))?;
    let naive = if end_of_day {
        date.and_hms_opt(23, 59, 59)
    } else {
        date.and_hms_opt(0, 0, 0)
    }
    .ok_or_else(|| format!("日期格式无效：{trimmed}"))?;

    Local
        .from_local_datetime(&naive)
        .single()
        .or_else(|| Local.from_local_datetime(&naive).earliest())
        .map(Some)
        .ok_or_else(|| format!("无法解析日期：{trimmed}"))
}

fn normalize_target_branch(value: &str) -> String {
    let trimmed = value.trim().trim_matches('/');
    let without_head_prefix = trimmed.strip_prefix("refs/heads/").unwrap_or(trimmed);
    let without_origin_prefix = without_head_prefix
        .strip_prefix("origin/")
        .unwrap_or(without_head_prefix);

    if without_origin_prefix.is_empty() {
        DEFAULT_TARGET_BRANCH.to_string()
    } else {
        without_origin_prefix.to_string()
    }
}

fn start_of_week(now: chrono::DateTime<Local>) -> Result<chrono::DateTime<Local>, String> {
    let days_from_monday = now.weekday().num_days_from_monday() as i64;
    let date = now.date_naive() - ChronoDuration::days(days_from_monday);
    let naive = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "无法计算本周开始时间".to_string())?;

    Local
        .from_local_datetime(&naive)
        .single()
        .or_else(|| Local.from_local_datetime(&naive).earliest())
        .ok_or_else(|| "无法计算本周开始时间".to_string())
}

fn scan_roots(scan_dirs: &[String], issues: &mut Vec<ScanIssue>) -> Vec<PathBuf> {
    let mut repos = HashSet::new();

    for dir in scan_dirs {
        let root = PathBuf::from(dir);
        if !root.exists() {
            issues.push(ScanIssue {
                path: Some(dir.clone()),
                message: "扫描目录不存在，请重新选择".to_string(),
            });
            continue;
        }
        if !root.is_dir() {
            issues.push(ScanIssue {
                path: Some(dir.clone()),
                message: "扫描目录不是文件夹".to_string(),
            });
            continue;
        }

        let deadline = Instant::now() + ROOT_SCAN_TIMEOUT;
        collect_repos(&root, &root, 0, deadline, &mut repos, issues);
    }

    let mut result: Vec<PathBuf> = repos.into_iter().collect();
    result.sort();
    result
}

fn collect_repos(
    root: &Path,
    current: &Path,
    depth: usize,
    deadline: Instant,
    repos: &mut HashSet<PathBuf>,
    issues: &mut Vec<ScanIssue>,
) {
    if Instant::now() > deadline {
        issues.push(ScanIssue {
            path: Some(root.to_string_lossy().into_owned()),
            message: "扫描目录超时，已跳过剩余内容".to_string(),
        });
        return;
    }

    if depth > MAX_SCAN_DEPTH {
        return;
    }

    let metadata = match fs::symlink_metadata(current) {
        Ok(metadata) => metadata,
        Err(_) => {
            issues.push(ScanIssue {
                path: Some(current.to_string_lossy().into_owned()),
                message: "无权访问该目录，请检查权限".to_string(),
            });
            return;
        }
    };

    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return;
    }

    if depth > 0 && should_skip_dir(current) {
        return;
    }

    if current.join(".git").exists() {
        let repo_path = fs::canonicalize(current).unwrap_or_else(|_| current.to_path_buf());
        repos.insert(repo_path);
    }

    let entries = match fs::read_dir(current) {
        Ok(entries) => entries,
        Err(_) => {
            issues.push(ScanIssue {
                path: Some(current.to_string_lossy().into_owned()),
                message: "无权访问该目录，请检查权限".to_string(),
            });
            return;
        }
    };

    for entry in entries.flatten() {
        collect_repos(root, &entry.path(), depth + 1, deadline, repos, issues);
    }
}

fn should_skip_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| SKIP_DIRS.iter().any(|skip| *skip == name))
        .unwrap_or(false)
}

fn scan_repo(
    repo: &Path,
    emails: &HashSet<String>,
    since: &str,
    until: &str,
) -> Result<Vec<CommitRecord>, GitCommandError> {
    let repo_path = repo.to_string_lossy().into_owned();
    let repo_name = repo
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repository")
        .to_string();

    let args = vec![
        "-C".to_string(),
        repo_path.clone(),
        "log".to_string(),
        "--since".to_string(),
        since.to_string(),
        "--until".to_string(),
        until.to_string(),
        "--no-merges".to_string(),
        "--date=iso-strict".to_string(),
        format!("--format=format:{COMMIT_MARKER}%x1f%H%x1f%an%x1f%ae%x1f%aI%x1f%s"),
        "--numstat".to_string(),
    ];

    let output = run_git_owned(&args, GIT_COMMAND_TIMEOUT)?;
    let raw_commits = parse_git_log(&output);
    let mut commits = Vec::new();

    for commit in raw_commits {
        if !emails.contains(&commit.author_email.trim().to_lowercase()) {
            continue;
        }

        commits.push(CommitRecord {
            repo_name: repo_name.clone(),
            repo_path: repo_path.clone(),
            hash: commit.hash,
            author_name: commit.author_name,
            author_email: commit.author_email,
            author_date: commit.author_date,
            subject: commit.subject,
            additions: Some(commit.additions),
            deletions: Some(commit.deletions),
            files: commit.files,
        });
    }

    Ok(commits)
}

fn scan_repo_merges(
    repo: &Path,
    target_branch: &str,
    since: &str,
    until: &str,
) -> Result<Vec<MergedBranchRecord>, GitCommandError> {
    let repo_path = repo.to_string_lossy().into_owned();
    let repo_name = repo
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repository")
        .to_string();
    let target_ref = resolve_target_ref(&repo_path, target_branch)?;

    let args = vec![
        "-C".to_string(),
        repo_path.clone(),
        "log".to_string(),
        target_ref.clone(),
        "--since".to_string(),
        since.to_string(),
        "--until".to_string(),
        until.to_string(),
        "--merges".to_string(),
        "--first-parent".to_string(),
        "--date=iso-strict".to_string(),
        format!("--format=format:{MERGE_MARKER}%x1f%H%x1f%cn%x1f%ce%x1f%cI%x1f%s"),
    ];

    let output = run_git_owned(&args, GIT_COMMAND_TIMEOUT)?;
    let raw_merges = parse_git_merges(&output);
    let mut merges = Vec::new();

    for merge in raw_merges {
        merges.push(MergedBranchRecord {
            repo_name: repo_name.clone(),
            repo_path: repo_path.clone(),
            target_branch: target_branch.to_string(),
            target_ref: target_ref.clone(),
            source_branch: infer_source_branch(&merge.subject),
            hash: merge.hash,
            committer_name: merge.committer_name,
            committer_email: merge.committer_email,
            merged_at: merge.merged_at,
            subject: merge.subject,
        });
    }

    Ok(merges)
}

fn resolve_target_ref(repo_path: &str, target_branch: &str) -> Result<String, GitCommandError> {
    let remote_ref = format!("refs/remotes/origin/{target_branch}");
    if git_ref_exists(repo_path, &remote_ref)? {
        return Ok(format!("origin/{target_branch}"));
    }

    let local_ref = format!("refs/heads/{target_branch}");
    if git_ref_exists(repo_path, &local_ref)? {
        return Ok(target_branch.to_string());
    }

    Err(GitCommandError::Exit(format!(
        "未找到目标分支 origin/{target_branch} 或 {target_branch}，已跳过合并扫描"
    )))
}

fn git_ref_exists(repo_path: &str, git_ref: &str) -> Result<bool, GitCommandError> {
    let args = vec![
        "-C".to_string(),
        repo_path.to_string(),
        "rev-parse".to_string(),
        "--verify".to_string(),
        "--quiet".to_string(),
        git_ref.to_string(),
    ];

    match run_git_owned(&args, Duration::from_secs(1)) {
        Ok(_) => Ok(true),
        Err(GitCommandError::Exit(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

fn parse_git_log(output: &str) -> Vec<RawCommit> {
    let mut commits = Vec::new();
    let mut current: Option<RawCommit> = None;

    for line in output.lines() {
        if let Some(payload) = line.strip_prefix(COMMIT_MARKER) {
            if let Some(commit) = current.take() {
                commits.push(commit);
            }

            let fields: Vec<&str> = payload
                .trim_start_matches('\u{1f}')
                .split('\u{1f}')
                .collect();
            if fields.len() >= 5 {
                current = Some(RawCommit {
                    hash: fields[0].to_string(),
                    author_name: fields[1].to_string(),
                    author_email: fields[2].to_string(),
                    author_date: fields[3].to_string(),
                    subject: fields[4..].join(" "),
                    additions: 0,
                    deletions: 0,
                    files: Vec::new(),
                });
            }
            continue;
        }

        let Some(commit) = current.as_mut() else {
            continue;
        };

        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() < 3 {
            continue;
        }

        let path = parts[2].trim();
        if path.is_empty() {
            continue;
        }

        let additions = parse_numstat_count(parts[0]);
        let deletions = parse_numstat_count(parts[1]);

        if let Some(value) = additions {
            commit.additions += value;
        }

        if let Some(value) = deletions {
            commit.deletions += value;
        }

        commit.files.push(CommitFileChange {
            path: normalize_numstat_path(path),
            additions,
            deletions,
        });
    }

    if let Some(commit) = current.take() {
        commits.push(commit);
    }

    commits
}

fn parse_numstat_count(value: &str) -> Option<u64> {
    let value = value.trim();
    if value == "-" {
        return None;
    }

    value.parse::<u64>().ok()
}

fn normalize_numstat_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

fn parse_git_merges(output: &str) -> Vec<RawMerge> {
    let mut merges = Vec::new();

    for line in output.lines() {
        let Some(payload) = line.strip_prefix(MERGE_MARKER) else {
            continue;
        };

        let fields: Vec<&str> = payload
            .trim_start_matches('\u{1f}')
            .split('\u{1f}')
            .collect();
        if fields.len() < 5 {
            continue;
        }

        merges.push(RawMerge {
            hash: fields[0].to_string(),
            committer_name: fields[1].to_string(),
            committer_email: fields[2].to_string(),
            merged_at: fields[3].to_string(),
            subject: fields[4..].join(" "),
        });
    }

    merges
}

fn infer_source_branch(subject: &str) -> Option<String> {
    let branch = quoted_branch_after(subject, "Merge branch '")
        .or_else(|| quoted_branch_after(subject, "Merge remote-tracking branch '"))
        .or_else(|| branch_after_prefix(subject, "Merge pull request ", " from "))
        .or_else(|| branch_after_prefix(subject, "Merged in ", "Merged in "));

    branch
        .map(clean_source_branch)
        .filter(|value| !value.is_empty())
}

fn quoted_branch_after(subject: &str, prefix: &str) -> Option<String> {
    let rest = subject.strip_prefix(prefix)?;
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

fn branch_after_prefix(subject: &str, required_prefix: &str, marker: &str) -> Option<String> {
    if !subject.starts_with(required_prefix) {
        return None;
    }

    let start = if required_prefix == marker {
        required_prefix.len()
    } else {
        subject.find(marker)? + marker.len()
    };
    let rest = subject[start..].trim();
    let end = rest
        .find(|character: char| character.is_whitespace() || character == '(')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn clean_source_branch(value: String) -> String {
    value
        .trim()
        .trim_matches('\'')
        .trim_start_matches("refs/heads/")
        .trim_start_matches("refs/remotes/")
        .trim_start_matches("origin/")
        .to_string()
}

fn build_report(
    commits: Vec<CommitRecord>,
    merged_branches: Vec<MergedBranchRecord>,
    issues: Vec<ScanIssue>,
    scanned_at: String,
    week_start: String,
    week_end: String,
    target_branch: String,
) -> WeeklyReport {
    let total_additions = commits.iter().filter_map(|commit| commit.additions).sum();
    let total_deletions = commits.iter().filter_map(|commit| commit.deletions).sum();
    let repo_count = commits
        .iter()
        .map(|commit| commit.repo_path.as_str())
        .chain(merged_branches.iter().map(|merge| merge.repo_path.as_str()))
        .collect::<HashSet<_>>()
        .len() as u64;
    let merged_branch_count = merged_branches.len() as u64;

    WeeklyReport {
        summary: Summary {
            total_commits: commits.len() as u64,
            total_additions,
            total_deletions,
            repo_count,
            merged_branch_count,
        },
        commits,
        merged_branches,
        issues: compact_issues(issues),
        scanned_at,
        week_start,
        week_end,
        target_branch,
    }
}

fn compact_issues(issues: Vec<ScanIssue>) -> Vec<ScanIssue> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for issue in issues {
        let key = format!(
            "{}::{}",
            issue.path.as_deref().unwrap_or_default(),
            issue.message
        );
        if seen.insert(key) {
            result.push(issue);
        }
    }

    result
}

fn run_git(args: &[&str], timeout: Duration) -> Result<String, GitCommandError> {
    let owned_args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    run_git_owned(&owned_args, timeout)
}

fn run_git_owned(args: &[String], timeout: Duration) -> Result<String, GitCommandError> {
    let mut command = Command::new("git");
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            GitCommandError::Missing
        } else {
            GitCommandError::Io(error.to_string())
        }
    })?;

    match child
        .wait_timeout(timeout)
        .map_err(|error| GitCommandError::Io(error.to_string()))?
    {
        Some(_) => {
            let output = child
                .wait_with_output()
                .map_err(|error| GitCommandError::Io(error.to_string()))?;
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(GitCommandError::Exit(if stderr.is_empty() {
                    "未知错误".to_string()
                } else {
                    stderr
                }))
            }
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            Err(GitCommandError::Timeout)
        }
    }
}
