# 许可证来源审计清单

本目录所有许可证文件均从官方来源**原样复制**，未做任何手写/修改。
下表列出每个文件的原始下载链接，供审计核对。

| 文件 | 对应依赖 | 原始链接 |
|------|---------|---------|
| `../LICENSE`（项目根目录） | 本项目自身 | https://www.apache.org/licenses/LICENSE-2.0.txt |
| `LICENSE-webrtc` | WebRTC（AEC 核心算法） | https://webrtc.googlesource.com/src/+/refs/heads/main/LICENSE |
| `LICENSE-webrtc-audio-processing-crate` | webrtc-audio-processing (Rust 封装) | https://raw.githubusercontent.com/tonarino/webrtc-audio-processing/main/COPYING |
| `LICENSE-flexaudio` | flexaudio（音频采集） | https://raw.githubusercontent.com/Studio-Sadola/flexaudio/main/LICENSE |
| `THIRD_PARTY_NOTICES-flexaudio.md` | flexaudio 第三方声明 | https://raw.githubusercontent.com/Studio-Sadola/flexaudio/main/THIRD_PARTY_NOTICES.md |
| `LICENSE-cpal` | cpal（音频输出） | https://raw.githubusercontent.com/RustAudio/cpal/master/LICENSE |
| `LICENSE-axum` | axum | https://raw.githubusercontent.com/tokio-rs/axum/main/LICENSE |
| `LICENSE-tokio` | tokio | https://raw.githubusercontent.com/tokio-rs/tokio/master/LICENSE |
| `LICENSE-abseil-cpp` | abseil-cpp（WebRTC 内部依赖） | https://raw.githubusercontent.com/abseil/abseil-cpp/master/LICENSE |
| `LICENSE-MPL-2.0.txt` | symphonia（仅测试，dev-deps） | https://www.mozilla.org/media/MPL/2.0/index.815ca599c9df.txt |
| `LICENSE-rnnoise` | rnnoise（WebRTC 内置） | https://raw.githubusercontent.com/xiph/rnnoise/master/COPYING |
| `LICENSE-pffft` | pffft（WebRTC 内置 FFT） | https://raw.githubusercontent.com/marton78/pffft/master/LICENSE.txt |
| `LICENSE-ooura-fft` | ooura FFT（WebRTC 内置） | https://webrtc.googlesource.com/src/+/refs/heads/main/common_audio/third_party/ooura/LICENSE |
| `LICENSE-spl-sqrt-floor` | spl_sqrt_floor（WebRTC 内置） | https://webrtc.googlesource.com/src/+/refs/heads/main/common_audio/third_party/spl_sqrt_floor/LICENSE |
| `LICENSE-fft-olean` | Mark Olesen FFT（WebRTC 内置） | https://webrtc.googlesource.com/src/+/refs/heads/main/modules/third_party/fft/LICENSE |

## 审计方法

```powershell
# 逐一对比本地文件与官方原文是否一致
Invoke-WebRequest "<上表链接>" | Select-Object -Expand Content | Compare-Object (Get-Content .\LICENSE-xxx)
```

## 不受本项目 Apache-2.0 约束的组件

- **VB-CABLE**（`src-tauri/resources/vbcable/`）：Vincent Burel / VB-Audio 版权，
  Donationware，按官方条款原样分发，见 https://vb-audio.com/Services/licensing.htm
