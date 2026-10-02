# Voice input for the TextInput mic and the AmbientAssistant voice mode

This survey covers DESIGN.md 5.4.2, a requirement document section that requires voice input on all text inputs and on the ambient assistant. It also covers TextInput and AmbientAssistant, two console UI components defined in components.md. The target platform is Tauri desktop. Tauri is a framework for building desktop apps with a Rust backend and a web-based UI. On Windows, Tauri uses WebView2, a Chromium-based embedded browser from Microsoft. On macOS, it uses WKWebView, an embedded browser from Apple, with web reuse planned for later.

## 1. Web Speech API (`SpeechRecognition`)

- On Windows, using WebView2, Web Speech API works well. WebView2 is Chromium-based, so `SpeechRecognition` behaves the same as in desktop Chrome or Edge. Chrome 139 added an on-device mode, released in August 2025. Setting `processLocally: true` turns this mode on. In this mode no audio leaves the machine, and latency is lower. Each language pack is about 60 MB. The browser handles this download on its own.
- On macOS, using WKWebView, support is mixed. Safari itself supports `webkitSpeechRecognition` from Safari 14.1 onward. It needs a speech-recognition entitlement added to Info.plist, an app configuration file on macOS. The first use triggers an OS consent dialog. This dialog states that audio is sent to Apple's servers, so the feature is not fully on-device. Tauri's own maintainers say Web Speech API should work on macOS if the app adds the Info.plist entry. See [the Tauri discussion on this](https://github.com/tauri-apps/tauri/discussions/13460) and [the related Tauri issue](https://github.com/tauri-apps/tauri/issues/6208). A plain WKWebView, embedded in an app rather than in Safari itself, has a known history here. It can silently fail to prompt for mic access in some app types. This needs a throwaway spike to confirm behaviour, rather than an assumption.
- On Linux, using WebKitGTK, an open-source browser engine for GTK apps, speech recognition is unsupported. Work on support has not started. If the console ever ships on Linux desktop, TextInput's mic must be hidden or built as a plugin there.
- Network dependency: off the on-device path, `SpeechRecognition` streams audio to Google's servers from Chrome, or to Apple's servers from Safari. This is a real concern for an ops console, since a text field may show credentials, tokens, or customer data.

## 2. Local / on-device STT

- **whisper.cpp**, a C++ implementation of OpenAI's Whisper speech-recognition model, can run via Rust as a Tauri sidecar or an in-process binding. It uses the whisper-rs crate, a Rust binding for whisper.cpp. Several shipped Tauri 2.x apps do this already, including Whisperi, Whisper Desktop, and MumbleFlow. The approach is mature. It needs no separate runtime install, since it static-links whisper.cpp. Model footprint varies by size. The `tiny` and `base` models run about 75 to 150 MB. The `small` model runs about 500 MB. `base.en` is the realistic default for a snappy console, since it runs near-real-time on CPU and faster still with GPU, Metal, or DirectML acceleration. First-token latency after audio capture runs roughly 200 ms to 1 s, depending on model size and hardware. This is good enough for dictation. It is borderline for a fluid open-mic assistant.
- **Transformers.js**, a JavaScript library that runs machine-learning models, including Whisper, in the browser, can pair with **WebGPU**, a browser API for GPU-accelerated compute. Together they run Whisper inference directly inside the webview, without a Rust sidecar. WebGPU gives a 5 to 10x speedup over WASM for Whisper inference. Chrome and Edge from version 113 onward support WebGPU, which covers WebView2. This approach still needs a model download of several hundred MB on first run. WebGPU support inside WKWebView is far less proven than inside WebView2. In practice, treat this option as Windows-only today.
- Both local paths are genuinely private, since no audio crosses the network, and both work offline. This is a real advantage for an ops console. The cost is engineering and packaging effort, including model bundling, model download, and GPU driver variance. Local paths are also slower to first word than cloud streaming STT.

## 3. Cloud streaming STT

- **Deepgram Nova-3** is a cloud streaming speech-to-text model from Deepgram, a cloud speech-to-text vendor. It returns partial transcripts in well under 300 ms. Pairing Nova-3 with Flux, an end-of-turn detection add-on, keeps that detection under 300 ms too. This makes it the current latency leader for conversational use. Batch pricing runs about $0.0043 per minute. Streaming is metered separately, at roughly a few cents per hour of active use.
- **AssemblyAI Universal-3 Pro** is a competing cloud speech-to-text model, from vendor AssemblyAI. It runs about 760 ms time-to-final on mixed audio. Streaming costs about $0.45 per hour. It has strong keyterm-prompting support, useful for injecting our domain vocabulary such as work item ids and gate names.
- **OpenAI's gpt-realtime and whisper-class models** stream at about $0.017 per minute, or roughly $1 per hour. This is convenient if the console already runs an agent stack based on OpenAI. It adds a second network dependency and a second vendor.
- **Azure and Google STT** sit in a comparable latency and cost band to the options above. Azure is the same backend that Windows' own Voice Typing, the built-in OS dictation feature, already uses.
- Privacy trade-off is the deciding factor for this product. Everything typed near a work item can include stack traces, secrets, or internal hostnames. Sending that audio to a third party by default conflicts with the console's job of handling sensitive code. This only works with an explicit opt-in, a redaction step, or a deployment where the enterprise already sends code to its LLM vendor.

## 4. OS-level dictation as a fallback, without integration

- **Windows Voice Typing**, opened with Win+H, a keyboard shortcut, is system-wide. It is cloud-backed by Azure Speech, with auto-punctuation. On Copilot+ PCs, a feature called Fluid Dictation adds local-AI cleanup. Voice Typing types into any focused field, including TextInput, with zero app code.
- **macOS Dictation** works the same way. It is system-wide, and the user invokes it with the Fn key or a menu.
- **What it gives us**: a free, zero-maintenance fallback that always works if the OS supports it.
- **What it can't give us** is control over the experience. There is no in-app mic button and no recording state. There is no interim-transcript rendering inside TextInput, and no way to theme or brand it. There is no programmatic start or stop. The app cannot distinguish a user who is dictating from a user who is typing, in its own state model. It cannot satisfy the DESIGN.md 5.4.2 requirement for voice as a visible, first-class input mode, since it stays invisible to the app by construction. It is a fine fallback for users who discover it themselves. It is not the mic affordance itself.

## 5. UX patterns

- **Push-to-talk with an interim/final split** is the converged pattern. The user taps the mic, which starts a recording state with an animated stop control. Interim results then replace each other live in the field. A final result commits the text. assistant-ui, an open-source component library for chat and voice AI interfaces, ships exactly this pattern. Its `ComposerPrimitive.Dictate` and `StopDictation` primitives back this by default with Web Speech API. They also expose a pluggable `DictationAdapter` for server-side backends such as Whisper or ElevenLabs, a voice-AI vendor. This adapter-seam idea is worth copying, so the console can swap Web Speech, local whisper.cpp, or cloud STT behind one interface.
- **Consumer dictation apps**, such as Wispr Flow, a paid cloud dictation app, and Superwhisper, show the ceiling for this UX. Wispr Flow reformats rambling speech into clean text. It runs in the cloud, works cross-platform, costs about $18 per month, and has mixed reliability reports on Windows. Superwhisper is Mac-only, fully local, and needs no cloud connection. Together they show that local-first, with no reformatting magic, is a legitimate and privacy-respecting default rather than a compromise.
- **Error and permission states to design for** include several cases. Mic permission can be denied. The network can be down on the cloud path. A model can still be downloading on the local path. Speech can go undetected. A recognition error can happen mid-session. assistant-ui models this as a three-way session end state, using `stopped`, `cancelled`, or `error`. This maps cleanly onto DESIGN.md's state catalog in section 16.1, which is loading, streaming, error, or empty.

## Comparison

| Option | Latency | Privacy | Cost | Tauri compatibility | Effort |
|---|---|---|---|---|---|
| Web Speech API (cloud mode) | Low to medium | Audio leaves device (Google/Apple) | Free | Windows: good. macOS: works, needs Info.plist + OS consent dialog. Linux: unsupported | Very low |
| Web Speech API (on-device, Chrome 139+) | Low | Fully local | Free | Windows/WebView2 only today. About 60 MB language pack | Low |
| whisper.cpp via Tauri sidecar/Rust | Medium, 200 ms to 1 s | Fully local | Free (compute only) | All desktop platforms uniformly | Medium to high |
| Transformers.js + WebGPU (in-webview) | Medium, GPU-dependent | Fully local | Free | Solid on WebView2. Unproven on WKWebView | Medium |
| Cloud streaming STT (Deepgram/AssemblyAI/OpenAI) | Very low, sub-300 ms to 1 s | Audio leaves device to vendor | About $0.005 to $0.02/min | Platform-agnostic (network call) | Low to medium |
| OS dictation, Win+H and macOS Dictation | Low | Varies (often cloud) | Free | Works everywhere, but invisible to app | None (but no affordance) |

## Recommendation

- **TextInput mic, the default on Tauri desktop,** uses Web Speech API in on-device or local mode where available. This covers Windows/WebView2 with Chrome 139 or newer. It automatically falls back to whisper.cpp via sidecar when on-device Web Speech is not available. That covers macOS WKWebView today, and older WebView2 runtimes. This keeps the default fully local, which suits a console that may show secrets. It also avoids forcing a Rust sidecar dependency on Windows, the one platform where the browser API already handles this.
- **Fallback chain**, in order:

  1. Web Speech API on-device.
  2. whisper.cpp sidecar, using the bundled `base.en` model.
  3. Web Speech API cloud mode, opt-in only, with a visible notice that audio leaves this device.
  4. OS-level dictation, as the silent, always-available last resort the user can invoke themselves. This has no in-app affordance.

  Cloud streaming STT, such as Deepgram or AssemblyAI, is not in the default chain. It is a candidate only for a deployment that already accepts sending code or data to an LLM vendor. Even then, it stays admin-configurable, off by default.
- **AmbientAssistant voice mode** has different constraints. It is conversational, either open-mic or hold-to-talk. It is latency-sensitive. It is often already paired with a cloud LLM backend. Default to cloud streaming STT, Deepgram Nova-3 class, for the assistant's voice mode when the assistant itself is cloud-backed. Turn-detection quality matters more there than in a text field, and the assistant already implies a network round-trip. Fall back to whisper.cpp locally when the assistant runs against a local model, or when the operator has opted out of cloud.
- **One abstraction, partial pass.** A single `VoiceInputAdapter` interface can front both TextInput and AmbientAssistant. It needs start and stop controls, interim and final events, and error and permission states. This is the right seam, and it mirrors assistant-ui's `DictationAdapter`. The backend selected behind that interface should differ by default. TextInput defaults to local or on-device, for privacy and simplicity. AmbientAssistant defaults to cloud streaming, for turn-taking quality, when it is already cloud-backed. The console should build one component and ship it with two different default configurations.

## Sources

- [Tauri discussion on Web Speech API support](https://github.com/tauri-apps/tauri/discussions/13460)
- [Can I WebView… Speech recognition feature support](https://caniwebview.com/features/web-feature-speech-recognition/)
- [On-Device Speech UIs in Chrome 139 (Medium)](https://medium.com/@roman_fedyskyi/on-device-speech-uis-in-chrome-139-4b9f0397b9c9)
- [Chromium Intent to Ship: On-device Web Speech API](https://groups.google.com/a/chromium.org/g/blink-dev/c/VNOok2dbmHM/m/gwbtzV-lAQAJ)
- [web-speech-api on-device explainer (WebAudio/web-speech-api)](https://github.com/WebAudio/web-speech-api/blob/main/explainers/on-device-speech-recognition.md)
- [tauri-plugin-stt, a whisper-rs based Tauri plugin](https://github.com/brenogonzaga/tauri-plugin-stt)
- [Local Voice-to-Text App with Rust, Tauri 2.0, whisper.cpp (DEV Community)](https://dev.to/auratech/i-built-a-local-voice-to-text-app-with-rust-tauri-20-whispercpp-and-llamacpp-heres-how-32h5)
- [Whisper WebGPU vs WASM performance benchmark, filed as a transformers.js issue](https://github.com/huggingface/transformers.js/issues/894)
- [Deepgram vs Google vs AssemblyAI 2026 comparison](https://deepgram.com/learn/deepgram-vs-assemblyai-vs-whisper)
- [Speech-to-Text APIs in 2026: Benchmarks, Pricing (FutureAGI)](https://futureagi.com/blog/speech-to-text-apis-in-2026-benchmarks-pricing-developer-s-decision-guide/)
- [assistant-ui: Dictation guide](https://www.assistant-ui.com/docs/guides/dictation)
- [Best Dictation Tools for Windows in 2026 (OpenWhispr)](https://openwhispr.com/blog/best-dictation-tools-windows-2026)
- [Wispr Flow vs Superwhisper vs MacWhisper: 2026 (Spokenly)](https://spokenly.app/blog/wispr-flow-vs-superwhisper-vs-macwhisper)
- [Is webkitSpeechRecognition still unsupported? (Microsoft Q&A, WebView2)](https://learn.microsoft.com/en-us/answers/a/2054895)
