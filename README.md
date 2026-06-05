# GitSage

Tauri + Vue3 + Tailwind 实现的本地 Git 工作洞察与报告工具。

## 开发

```bash
npm install
npm run tauri:dev
```

## 构建

完整平台默认打包：

```bash
npm run tauri:build
```

macOS 仅生成 `.app`：

```bash
npm run tauri:build:mac-app
```

当前项目默认支持 macOS / Windows。macOS 的完整默认打包会尝试生成 `.app` 和 `.dmg`；如果在沙箱或无 Finder/AppleScript 权限的环境中失败，可先使用 `tauri:build:mac-app` 验证应用包。
