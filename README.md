# Echo AEC - 麦克风回声消除软件

基于 WebRTC AEC3 的实时回声消除工具。通过虚拟音频设备将处理后的无回声麦克风信号提供给系统使用。

## 架构

```
真实麦克风 → flexaudio 采集 → WebRTC AEC3 (process_capture_frame) → cpal 输出到虚拟麦克风
系统音频   → flexaudio WASAPI Loopback → WebRTC AEC3 (process_render_frame)
```

- 后端：Rust + axum（HTTP/WebSocket API，端口 3000）
- 前端：React + Vite + TailwindCSS（端口 5173）

## 运行

### 前置要求

1. **虚拟音频设备驱动**：安装 [Scream](https://github.com/duncanthrax/scream)（Windows 虚拟声卡，MS-PL 许可证）
2. **Rust 工具链**：https://rustup.rs
3. **构建工具**（编译 webrtc-audio-processing 需要）：
   - Visual Studio Build Tools（C++ 桌面开发）
   - Python 3 + `pip install meson`
   - Ninja（VS 自带或 `winget install Ninja-build.Ninja`）
   - Git for Windows（提供 `cp` 命令）
4. **Node.js**：前端开发

### 编译后端

由于 webrtc-audio-processing 在 Windows 上编译需要特殊环境（MSVC + meson + ninja + libclang + C++20），使用项目自带的构建脚本：

```bat
C:\echo-aec\build.bat
```

### 运行

```bat
# 后端
C:\echo-aec\audio-core\target\release\echo-aec-core.exe

# 前端（开发模式）
cd C:\echo-aec\web-ui
npm install
npm run dev
```

打开 http://localhost:5173 ，选择：
1. 麦克风（真实输入设备）
2. Loopback 设备（要监听的输出设备，如 HDMI/扬声器）
3. 虚拟麦克风（Scream 设备）

点击"启动 AEC"，然后在会议软件中选择 Scream 作为麦克风。

## 技术要点

- WebRTC AEC3 内部自动管理 render buffer 和延迟估计，loopback 线程直接调用 `process_render_frame`，mic 线程直接调用 `process_capture_frame`
- 不要手动管理参考信号的偏移/延迟，会与 AEC 内部延迟估计冲突
- AEC 处理延迟约 9-10ms，评估效果时需考虑

## 依赖与许可证

### Rust 后端依赖

| 依赖 | 版本 | 用途 | 许可证 |
|------|------|------|--------|
| [webrtc-audio-processing](https://github.com/tonarino/webrtc-audio-processing) | ~2.0 | WebRTC AEC3 回声消除（核心算法） | 见下方说明* |
| [flexaudio](https://github.com/Studio-Sadola/flexaudio) | 0.2 | 麦克风采集 + WASAPI Loopback | MIT |
| [cpal](https://github.com/RustAudio/cpal) | 0.15 | 音频输出到虚拟设备 | Apache-2.0 |
| [axum](https://github.com/tokio-rs/axum) | 0.7 | HTTP/WebSocket 服务 | MIT |
| [tokio](https://github.com/tokio-rs/tokio) | 1 | 异步运行时 | MIT |
| [tower-http](https://github.com/tower-rs/tower-http) | 0.5 | CORS 中间件 | MIT |
| [serde](https://github.com/serde-rs/serde) / serde_json | 1 | JSON 序列化 | MIT OR Apache-2.0 |
| [anyhow](https://github.com/dtolnay/anyhow) | 1 | 错误处理 | MIT OR Apache-2.0 |
| [tracing](https://github.com/tokio-rs/tracing) / tracing-subscriber | 0.1 / 0.3 | 日志 | MIT |
| [parking_lot](https://github.com/Amanieu/parking_lot) | 0.12 | 互斥锁 | MIT OR Apache-2.0 |
| [futures-util](https://github.com/rust-lang/futures-rs) | 0.3 | 异步工具 | MIT OR Apache-2.0 |
| [symphonia](https://github.com/pdeljanov/Symphonia) | 0.5 | 音频文件解码（仅测试用） | MPL-2.0 |

\* **webrtc-audio-processing 许可证说明**：该 crate 封装了 WebRTC 的 AudioProcessing 模块。WebRTC 代码使用 [BSD 风格许可证](https://webrtc.org/support/license)。上传 git 时需要附带 WebRTC 的 LICENSE 文件。

### 前端依赖

| 依赖 | 用途 | 许可证 |
|------|------|--------|
| [React](https://github.com/facebook/react) | UI 框架 | MIT |
| [Vite](https://github.com/vitejs/vite) | 构建工具 | MIT |
| [TailwindCSS](https://github.com/tailwindlabs/tailwindcss) | CSS 框架 | MIT |

### 外部运行时依赖

| 软件 | 用途 | 许可证 |
|------|------|--------|
| [Scream](https://github.com/duncanthrax/scream) | Windows 虚拟声卡驱动 | MS-PL (Microsoft Public License) |

### 参考项目

- [PipeWire](https://gitlab.freedesktop.org/pipewire/pipewire) - Linux 下的同类音频处理管线，本项目参考了其 WebRTC AEC 配置方式（MIT 许可证）

## 项目结构

```
echo-aec/
├── audio-core/          # Rust 后端
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs      # HTTP 服务入口
│       ├── api.rs       # REST API + WebSocket
│       ├── device.rs    # 设备枚举
│       └── audio/
│           ├── mod.rs
│           ├── aec.rs   # WebRTC AEC 封装
│           └── engine.rs # 音频引擎（3 线程）
├── web-ui/              # React 前端
│   └── src/App.jsx      # 设备选择 UI
├── build.bat            # Windows 构建脚本
└── README.md
```
