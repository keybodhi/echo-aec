# Echo AEC - 麦克风回声消除软件

基于 WebRTC AEC3 的实时回声消除工具（Windows 桌面应用）。通过虚拟音频设备将处理后的无回声麦克风信号提供给系统使用。

## 架构

```
真实麦克风 → flexaudio 采集 → WebRTC AEC3 (process_capture_frame) → cpal 输出到虚拟麦克风
系统音频   → flexaudio WASAPI Loopback → WebRTC AEC3 (process_render_frame)
```

- 应用框架：Tauri v2（Rust 后端 + WebView2 前端，单 .exe）
- 前端：React + Vite + TailwindCSS

## 快速开始（用户篇）

### 第一步：安装 VB-CABLE 虚拟声卡

VB-CABLE 是带数字签名的虚拟音频驱动，安装简单、无需关闭 Secure Boot。

1. 打开官网下载页：**https://vb-audio.com/Cable/**
2. 点击 **Download** 下载 `VBCABLE_Driver_Packxx.zip`
3. 解压 ZIP 到任意目录
4. 右键 **`VBCABLE_Setup_x64.exe`** → **以管理员身份运行**
5. 点击 **Install Driver**，等待安装完成
6. **重启电脑**（建议，确保设备正常加载）

安装后系统中会出现两个新设备：
- **CABLE Input (VB-Audio Virtual Cable)** —— 播放设备（我们的程序往这里写音频）
- **CABLE Output (VB-Audio Virtual Cable)** —— 录音设备（会议软件从这里读音频）

> VB-CABLE 是免费软件（Donationware，可自愿捐款）。

### 第二步：运行 Echo AEC

1. 运行 `echo-aec.exe`
2. 选择三个设备（选择一次后自动保存，下次启动自动恢复）：
   - **麦克风**：你的真实麦克风
   - **系统音频监听 (Loopback)**：你实际听声音的输出设备（扬声器 / HDMI 显示器音频等）
   - **虚拟麦克风**：选 **CABLE Input (VB-Audio Virtual Cable)**
3. 点击 **启动 AEC**
4. 在会议软件（Zoom / Teams / 腾讯会议等）中，把麦克风选为 **CABLE Output**

### 托盘与设备记忆

- 关闭窗口会最小化到系统托盘；右键托盘图标可"显示窗口"或"退出"
- 设备选择保存在 exe 同级的 `config.json`；若上次的设备已拔出，界面会提示

### 常见问题

| 问题 | 排查 |
|------|------|
| 没有回声消除效果 | 确认 Loopback 选的是**你真实听声音的输出设备**，不是 CABLE |
| 炸麦/啸叫 | ① Loopback 误选了 CABLE（形成反馈回路）② Windows 声音设置里开了"侦听此设备" ③ 扬声器音量过大失真 |
| 会议软件里没声音 | 会议软件麦克风选的是 **CABLE Output**（不是 CABLE Input） |
| 声音有延迟感 | AEC 处理延迟约 10ms 属正常，1-2 秒后延迟估计收敛效果最佳 |

## 编译（开发者篇）

### 环境要求

1. **Rust 工具链**：https://rustup.rs
2. **构建工具**（编译 webrtc-audio-processing 需要）：
   - Visual Studio Build Tools（C++ 桌面开发工作负载）
   - Python 3 + `pip install meson`
   - Ninja（VS 自带，或 `winget install Ninja-build.Ninja`）
   - Git for Windows（提供 `cp` 命令）
3. **Node.js**：前端构建

### 编译

由于 webrtc-audio-processing 在 Windows 上编译需要特殊环境（MSVC + meson + ninja + libclang + C++20），使用项目自带的构建脚本：

```bat
C:\echo-aec\tauri.bat build
```

产物：
- 可执行文件：`src-tauri\target\release\echo-aec.exe`
- 安装包：`src-tauri\target\release\bundle\`（MSI + NSIS）

开发模式（前端热重载）：

```bat
C:\echo-aec\tauri.bat dev
```

## 技术要点

- WebRTC AEC3 内部自动管理 render buffer 和延迟估计，loopback 线程直接调用 `process_render_frame`，mic 线程直接调用 `process_capture_frame`
- **不要手动管理参考信号的偏移/延迟**，会与 AEC 内部延迟估计冲突
- **不要强制 `stream_delay_ms` 固定值**，真实系统延迟约 8-16ms，强制错误值会导致滤波器发散（炸麦）
- AEC 处理延迟约 9-10ms，评估效果时需用互相关（lag 补偿）而非直接相关性

## 依赖与许可证

### Rust 后端依赖

| 依赖 | 版本 | 用途 | 许可证 |
|------|------|------|--------|
| [tauri](https://github.com/tauri-apps/tauri) | 2 | 桌面应用框架 | MIT OR Apache-2.0 |
| [webrtc-audio-processing](https://github.com/tonarino/webrtc-audio-processing) | ~2.0 | WebRTC AEC3 回声消除（核心算法） | BSD 3-Clause* |
| [flexaudio](https://github.com/Studio-Sadola/flexaudio) | 0.2 | 麦克风采集 + WASAPI Loopback | MIT |
| [cpal](https://github.com/RustAudio/cpal) | 0.15 | 音频输出到虚拟设备 | Apache-2.0 |
| [serde](https://github.com/serde-rs/serde) / serde_json | 1 | JSON 序列化 | MIT OR Apache-2.0 |
| [anyhow](https://github.com/dtolnay/anyhow) | 1 | 错误处理 | MIT OR Apache-2.0 |
| [tracing](https://github.com/tokio-rs/tracing) / tracing-subscriber | 0.1 / 0.3 | 日志 | MIT |
| [parking_lot](https://github.com/Amanieu/parking_lot) | 0.12 | 互斥锁 | MIT OR Apache-2.0 |
| [symphonia](https://github.com/pdeljanov/Symphonia) | 0.5 | 音频文件解码（仅测试用，dev-dependencies） | MPL-2.0 |

\* webrtc-audio-processing 封装了 WebRTC 的 AudioProcessing 模块（BSD 3-Clause）。分发时需附带 `licenses/` 目录下的许可证文件。

### 前端依赖

| 依赖 | 用途 | 许可证 |
|------|------|--------|
| [React](https://github.com/facebook/react) | UI 框架 | MIT |
| [Vite](https://github.com/vitejs/vite) | 构建工具 | MIT |
| [TailwindCSS](https://github.com/tailwindlabs/tailwindcss) | CSS 框架 | MIT |
| [@tauri-apps/api](https://github.com/tauri-apps/tauri) | 前端与后端通信 | MIT OR Apache-2.0 |

### 外部运行时依赖

| 软件 | 用途 | 许可证 |
|------|------|--------|
| [VB-CABLE](https://vb-audio.com/Cable/) | Windows 虚拟声卡驱动 | 免费软件（Donationware） |

### 参考项目

- [PipeWire](https://gitlab.freedesktop.org/pipewire/pipewire) - Linux 下的同类音频处理管线，本项目参考了其 WebRTC AEC 配置方式（MIT 许可证）

## 项目结构

```
echo-aec/
├── src-tauri/           # Tauri 桌面应用（Rust 后端）
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── icons/
│   └── src/
│       ├── main.rs      # 应用入口 + Tauri 命令 + 托盘
│       ├── config.rs    # 设备选择持久化 (config.json)
│       ├── device.rs    # 设备枚举
│       └── audio/
│           ├── mod.rs
│           ├── aec.rs   # WebRTC AEC 封装
│           └── engine.rs # 音频引擎（3 线程）
├── web-ui/              # React 前端
│   └── src/App.jsx      # 设备选择 UI（Tauri invoke）
├── licenses/            # 全部依赖的许可证文件（分发时必须包含）
├── audio-core/          # 旧版 HTTP 后端（已弃用，保留参考）
├── tauri.bat            # Windows 构建脚本
└── README.md
```
