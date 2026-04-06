# **花也 (Huaye) 架构文档**

本文档是花也从单文件脚本演进为通用工业级生产力工作站的技术指南。目标是建立一个稳固的底层基座，能够承载 CAN 总线分析、逻辑分析仪、设备 OTA、机器视觉等多种工业场景，同时保持严格的跨平台一致性。

## **壹、核心愿景与设计哲学**

设计哲学：

1. **模块化单体 (Modular Monolith)**：底座只处理抽象的数据流与视图，所有业务模块在编译期静态注册，兼顾性能与类型安全。  
2. **工业环境健壮性**：面对断电、超大日志、高频数据风暴，底座须具备容错与防撑爆能力。  
3. **可扩展**：未来考虑 Lua/WASM/Python 脚本引擎接入，适应自动化测试与产线流程。

## **贰、UI 框架选型：为什么死磕 egui？**

评估了 Tauri、Slint 等方案后，选择 egui：

* **否决 Tauri**：高频数据流在跨进程 IPC 序列化面前性能不可接受。  
* **否决 Slint**：生态不成熟，缺少高性能绘图库等关键组件。  
* **选择 egui (立即模式)**：直接读内存提交 GPU，适合大数据量绘图和 120Hz 刷新；生态有 egui\_plot 等工程组件。

**已知风险**：egui 每帧重算布局，复杂界面下 CPU 开销大。已通过渲染节流器（6ms 时间片）缓解，同时需严控同屏控件数量。

## **叁、目录结构：从“面条”到“模块” (Rust现代规范)**

本项目全面采用 Rust 2018 推荐的目录同名文件模块声明规范，不使用 mod.rs。以下为当前实际目录结构：

├── src/  
│   ├── main.rs                 \# 入口：初始化系统 Hook、Tokio 运行时、启动 eframe  
│   ├── app.rs                  \# 外壳 (App Shell)：物理窗口渲染、阴影、路由、事件分发  
│   ├── core.rs                 \# 核心基建模块声明文件 (替代旧的 core/mod.rs)  
│   ├── core/                   \# 核心基建目录：通用底层基建，严禁侵入具体 UI 业务  
│   │   ├── theme.rs            \# 序列化、主题配置、视觉常量  
│   │   ├── module.rs           \# AppModule trait 定义  
│   │   ├── events.rs           \# AppEvent 枚举 + GLOBAL_EVENT_TX 静态发送器  
│   │   ├── state.rs            \# GlobalState — 全局状态，只读传入模块  
│   │   ├── logger.rs           \# tracing 初始化、panic hook、按次日志文件  
│   │   ├── config.rs           \# AppConfig 持久化 (exe 同目录 huaye_config.json)  
│   │   └── data\_pipeline.rs    \# RingBuffer\<T\> 泛型环形缓冲区  
│   ├── components.rs           \# 纯组件模块声明文件  
│   ├── components/             \# 纯组件目录：高复用 UI 片段  
│   │   └── decorations.rs      \# Logo、窗口按钮、阴影绘制  
│   ├── modules.rs              \# 业务模块声明文件  
│   └── modules/                \# 业务模块目录  
│       ├── dashboard.rs        \# 仪表盘模块  
│       ├── settings.rs         \# 通用设置面板模块  
│       └── terminal.rs         \# 串口终端模块（三栏布局：配置/接收/发送）

**远景规划文件（尚未创建，待需求明确后实现）**：  
- `core/ai.rs` — AI 协作接入层  
- `core/ext_engine.rs` — 动态脚本沙盒引擎 (mlua / wasmtime / python)  
- `core/mmap_storage.rs` — 零拷贝大数据内存映射 (memmap2)  
- `components/data_viewer.rs` — Hex、JSON、波形等通用数据渲染器  
- `modules/oscilloscope.rs` — 示波器/波形分析模块

## **肆、核心底座代码实现：如何完美“解耦”？**

这是打牢地基的第一步，必须确保底座与业务物理分离。

### **1\. 模块化单体协议 (AppModule Trait)**

彻底放弃复杂的动态加载机制，改为业务模块**静态注入**。这也是为了支持**多产品线形态**：例如功能 A 写在 modules/a.rs 中，我们可以通过配置不同的 bin (二进制产物)，让二进制 1 包含功能 A，也可以轻松将功能 A 移植到二进制 2 中。所有业务模块必须遵守通用规矩，外壳只提供上下文，不关心模块具体内容。在主程序初始化时进行静态注册（例如 modules.push(Box::new(Oscilloscope::new()))）。

pub trait AppModule {  
    fn name(\&self) \-\> \&str;  
    fn icon(\&self) \-\> \&str;   
      
    // 渲染主函数：传入受限的 ui、全局状态、以及用于发消息的 tx 通道  
    fn show\_content(  
        \&mut self,   
        ui: \&mut egui::Ui,   
        state: \&crate::core::state::GlobalState,   
        tx: \&flume::Sender\<crate::core::events::AppEvent\>  
    );  
      
    // 状态栏通用提示钩子  
    fn status\_bar\_hint(\&self) \-\> \&str { "就绪" }  
      
    // 生命周期钩子，用于资源清理  
    fn on\_exit(\&mut self) {}  
}

**关于 UI 样式实时预览**：模块不直接接触 RenderContext。外壳在每帧 update 中通过 `ui.style_mut().visuals` 注入最新主题配置后再调用 `show_content`，因此拖拽滑块等实时预览不受影响。代价是模块不感知物理窗口边距，换来沙盒隔离。

### **2\. 构建高性能通用异步事件总线 (EventBus)**

使用高性能 flume 支持多生产者多消费者。不再局限于某种特定数据，而是采用抽象的消息体。

// 全局静态发送器，供任意位置（包括 panic hook）发送事件
pub static GLOBAL_EVENT_TX: OnceLock\<flume::Sender\<AppEvent\>\> = OnceLock::new();

pub enum AppEvent {
    // ── 已实现 ──────────────────────────────────────────────
    LogMessage(String),                                        // 异步日志消息
    UpdateTheme(ThemeConfig),                                  // 主题变更
    UpdateDragTransparent(bool),                               // 拖动透视开关
    ToastRequest { text: String, is_error: bool },             // 统一弹窗 (is_error 替代 LogLevel)
    FatalError(String),                                        // Panic 拦截后发出
    SysInfoUpdate { cpu\_usage: f32, mem\_usage: u64 },          // 后台系统监控推送
    DataReady,                                                 // 数据就绪信号 (占位)

    // ── 远景规划 (待实现) ────────────────────────────────────
    // GenericDataReceived { source: String, payload: Vec\<u8\> }, // 通用二进制数据流
    // AiResponse(String),                                       // AI 流式返回
    // RunCommand(String),                                       // 调用系统/沙盒指令
}

### **3\. 重构主程序 (MyApp) 作为“卡槽控制器”**

MyApp 变为底座卡槽，只关心怎么“插拔”和“调度”插件。

struct MyApp {
    modules: Vec\<Box\<dyn AppModule\>\>,
    current\_index: usize,
    event\_tx: flume::Sender\<AppEvent\>,
    event\_rx: flume::Receiver\<AppEvent\>,
    state: GlobalState,
    // 系统监控缓存（由后台线程每秒推送一次）
    cpu\_usage: f32,
    mem\_usage: u64,
    // UI 状态变量（动画、拖拽、Toast 列表等）
    is\_dragging\_window: bool,
    toasts: Vec\<ToastMessage\>,
    last\_theme: ThemeConfig,  // 上一帧主题，用于 diff 后批量注入 visuals
    // ...
}

impl eframe::App for MyApp {
    fn update(\&mut self, ctx: \&egui::Context, \_frame: \&mut eframe::Frame) {
        // 1\. 基于时间片的渲染节流器（已实现）
        //    每帧最多处理 6ms 事件，保证 120Hz 下 UI 渲染有充足余量
        let loop\_start = std::time::Instant::now();
        let time\_limit = std::time::Duration::from\_millis(6);
        while let Ok(event) = self.event\_rx.try\_recv() {
            self.handle\_event(event);
            if loop\_start.elapsed() > time\_limit {
                ctx.request\_repaint(); // 时间片耗尽，告知 eframe 下帧继续
                break;
            }
        }
        // 注：eframe 在通道空时自动等待，不会空转满载 CPU

        // 2\. 绘制外壳 (标题栏、阴影、拖拽区、主题 diff 注入)
        // ...

        // 3\. 动态路由：在安全画布内渲染当前选中的业务模块
        if let Some(module) = self.modules.get\_mut(self.current\_index) {
            module.show\_content(ui, \&self.state, \&self.event\_tx);
        }
    }
}

**设计权衡与风险**：

1. **一帧延迟**：模块通过事件修改全局状态，存在一帧延迟。实践中模块内部维护可变副本，修改完成后发事件同步全局（单向数据流），消灭死锁风险。  
2. **渲染节流 [已实现]**：主事件循环采用时间片轮转（`loop_start.elapsed() > 6ms → break`），剩余事件留在 flume 队列下帧处理，不丢数据。极高频场景未来可考虑批量打包或共享内存方案。  
3. **避免锁竞争**：严格贯彻单向数据流，禁止滥用 `Arc<RwLock<T>>`。

## **伍、工程基建**

### **4\. Ring Buffer 与降采样 (LTTB)**

* **场景**：工业仪器长时间运行产生海量数据点。  
* **方案**：循环缓冲淘汰旧数据，渲染前经 LTTB 降采样压缩至屏幕分辨率级别。
* **实现状态**：`RingBuffer<T>` 已在 `src/core/data_pipeline.rs` 中完整实现（`push`/`push_batch`/`as_slices`/`clear`），含单元测试。**LTTB 降采样尚未实现**，待示波器模块接入时补充。

### **5\. 串口终端模块 (Terminal)**

串口终端是首个完整的业务模块，采用 A/B/C 三栏自适应布局：

* **A 区（左侧配置栏）**：串口参数配置（端口、波特率、数据位、校验位、停止位）、收发显示模式切换（ASCII/HEX）、时间戳、CRLF 等选项。支持拖拽分隔条调节宽度。
* **B 区（右上接收区）**：接收数据展示，使用 `RingBuffer<T>` 作为底层缓冲，配合脏标记 (`rx_dirty`) + 渲染缓存 (`cached_rx_lines`) 避免每帧重新格式化。`ScrollArea::show_rows` 实现 O(1) 虚拟滚动，只渲染可见行。工具栏按钮（数据筛选、文字高亮、清空显示）引用主题标题栏按钮配色保持视觉一致。
* **C 区（右下发送区）**：文本输入框 + 发送按钮。输入框在内容区较矮时（≤3 行高）自动垂直居中光标，拖高后切回顶部对齐。支持单条/多条发送模式切换，Ctrl+Enter 快捷发送。

**后台串口线程模型**：通过 `flume` 双通道（`BackgroundCommand` / `BackgroundEvent`）与 UI 线程通信，严格遵守单向数据流。后台线程拥有串口所有权，UI 线程只发指令、收事件，不直接接触 `SerialPort`。`is_connecting` 标志防止并发双击打开。

### **6\. 数据持久化：Mmap 零拷贝 [规划中]**

* **方案**：引入 memmap2，将大文件直接映射进虚拟内存，避免自身内存占用。

### **7\. 日志与遥测 (Tracing) [已实现]**

* **方案**：集成 tracing，按启动次数创建日志文件（`huaye_YYYYMMDD_HHMMSS_PID.log`），存放于 exe 同目录 `logs/`。Panic 自动捕获并通过 `GLOBAL_EVENT_TX` 发送 `FatalError` 事件。  
* **自动清理**：每次启动时自动清理 7 天以上的旧日志文件，防止长期运行后日志目录无限膨胀。
* **实现位置**：`src/core/logger.rs`，含 panic hook、WorkerGuard 生命周期管理、ANSI 颜色码关闭。

## **陆、远景功能规划 [均未实现]**

### **7\. 全局命令面板 (Omnibar)**

Ctrl+P 模糊搜索模块路由及命令，实现全键盘操作。

### **8\. 多显示器 (Multiviewport)**

支持将模块拖出主窗口形成独立窗口。**风险**：多窗口并发读写 GlobalState 会引发锁竞争，需预留无锁或只读快照方案。

### **9\. 脚本引擎**

支持 Lua/WASM/Python，允许用户通过脚本解析私有协议、编写自动化流程。

### **10\. AI 协作 (Co-pilot)**

接入大模型 API（Function Calling），通过内部总线联动界面和数据。

## **柒、长期基建清单 [均未实现]**

以下为架构初期规划的基建能力，待地基稳定后按优先级逐步实现：

### **1\. 核心稳定性与系统运维**

* **自动/热更新 (OTA)**：支持后台静默下载，无缝替换二进制。  
* **全局异常防御 (Crash Defense)**：拦截全局崩溃，杜绝软件闪退，提供友好报错与 dump 收集。  
* **系统资源监视器 (Resource Monitor)**：状态栏常驻，实时监控 CPU/内存与帧率 (FPS)。  
* **无头模式 (Headless Mode)**：支持命令行 \--headless 启动，仅运行后端数据采集引擎，无缝转为服务器后台守护进程。

### **2\. 深度数据管控与工作流**

* **工作区隔离 (Workspaces)**：如同 IDE，不同工程（文件夹）记忆各自的协议配置、波特率、窗口布局。  
* **本地配置持久化 (Local Persistence)**：配置在后台防掉电定时自动落盘。  
* **通用导入与导出 (Import/Export)**：全平台支持标准格式（CSV/JSON/BIN/PCAP）导入导出，方便联动 MATLAB 等工具二次分析。  
* **撤销与恢复 (Undo/Redo)**：引入命令模式，复杂参数配置支持 Ctrl+Z 容错回滚。

### **3\. 交互与体验**

* **模块间数据交互 (Module IPC)**：模块 A 的事件可被模块 B 监听并响应。  
* **正则高亮终端**：正则提取、过滤、格式化 Hex/ASCII 文本。  
* **系统指令调用 (System Calls)**：打通 OS 命令（如 gcc 编译、avrdude 烧录）。  
* **全局快捷键与焦点管理**：可靠的键盘响应链路。  
* **通知中心 (Notification Center)**：后台堆叠、进度条与模态对话框。  
* **多语言国际化 (i18n)**：运行期热切换语言。  
* **脚本沙盒 (Sandbox)**：限制脚本权限，防止越权操作。

### **4\. 测试规范**

* **单元测试**：在 `.rs` 文件底部用 `#[cfg(test)] mod tests` 编写，release 编译自动剥离。  
* **不测 GUI 绘制**：将数据处理、转换、过滤等逻辑抽离为纯函数，测试纯函数。  
* **Mock 硬件依赖**：串口、网络等通过 Trait 抽象，测试中传入 Mock 对象。

## **捌、风险与已知难点**

### **工程难点**

* **Ring Buffer 非连续内存与 LTTB 冲突**：LTTB 要求连续内存，但 Ring Buffer 底层是两段切片。需渲染前拼接或改造为迭代器算法。  
* **WASM/脚本 FFI 性能**：Rust 与脚本间高频传大数据会因序列化耗尽 CPU。应限定脚本处理低频控制流。  
* **Tokio 僵尸任务**：子线程 panic 静默死亡。须包裹异常拦截，向总线发送 FatalError。

### **已知隐患**

* **纯自绘无边框丢失系统交互**：Windows Aero Snap 等分屏快捷键失效。当前已移除原生窗口模式，如遇严重兼容问题需重新评估。  
* **原生对话框必须在主线程**：`rfd::FileDialog` 不可在后台线程打开（macOS 崩溃 / Windows COM 异常），须在 `show_content` 中调用，仅将后续文件 I/O 送入后台。

## **玖、编码红线**

1. **禁止过度实现**：远景规划（AI、Mmap、WASM、Python 等）除非明确要求，禁止提前编写占位符或引入相关库。  
2. **禁止主线程阻塞**：`show_content` 内禁止 `std::fs::read`、网络请求等阻塞操作，一律送 Tokio 后台。  
3. **禁止无脑加锁**：优先 Clone 或 Channel，避免 `Arc<Mutex<T>>` 滥用。  
4. **模块规范**：不使用 `mod.rs`；新建 `.rs` 文件后立即在对应声明文件中添加 `pub mod`；变更后 `cargo check` 通过。

## **拾、架构修订记录**

### **v0.4.x — 向”绝对绿色便携”与”极致解耦”演进**

1. **拒绝环境污染 (Green Software)**：抛弃所有系统级的 `AppData` 或 `~/.config` 依赖。所有的配置文件 (`huaye_config.json`) 和日志文件 (`logs/`) **必须**直接保存在可执行文件 (exe/elf) 所在的当前运行目录下。实现真正的”删目录即卸载”，不留系统垃圾。
2. **防覆盖的按次日志策略**：废弃按天轮转日志的方案。日志文件按照应用启动次数创建，命名规范为 `huaye_YYYYMMDD_HHMMSS_HASH.log`，彻底杜绝多开应用时的日志写冲突与查阅困难。
3. **终结原生窗口妥协**：彻底移除 `use_native_window` 选项与相关的冗余适配代码。为了极致的跨平台一致性，全面拥抱纯自绘无边框 UI，拒绝引入任何平台特定的原生阴影组件 (如 `window-shadows`)，将跨平台故障率降至最低。
4. **强制行内测试规范**：贯彻 Rust 官方实践，将单元测试 `#[cfg(test)] mod tests` 直接写在核心业务文件的底部，不单独建立 `tests` 根目录存放单元测试，以此保证逻辑与测试的紧密内聚，且 release 编译时自动剥离。

### **v0.4.3+ — 精简与收敛**

5. **删除窗口持久化功能**：移除 `AppConfig` 中的 `window_width`、`window_height`、`window_maximized`、`persist_window_geometry` 字段及相关逻辑。窗口几何持久化增加了无谓的复杂度，eframe 本身不提供可靠的跨平台窗口位置恢复，此功能性价比极低，予以删除。`AppConfig` 现仅持久化 `theme`。
6. **统一 UI 线程 channel 为 try_send**：明确规范 `show_content`（即 UI 渲染帧）中所有 `tx` 调用必须使用非阻塞的 `try_send()`，杜绝因通道满而阻塞主线程导致卡帧。后台线程中可酌情使用阻塞 `send()`。
7. **修复 WorkerGuard 提前释放**：日志系统 `tracing_appender` 的 `WorkerGuard` 必须在 `main()` 函数作用域内持有至程序退出，否则非阻塞写入器会提前关闭，导致末尾日志丢失。

8. **日志命名优化 (防 Hash 碰撞)**：废弃旧版的 `rand_u16` 后缀生成日志（存在极小概率的高频启动文件名碰撞风险）。更新为使用 `chrono::Local::now()` 及操作系统进程 PID (`std::process::id()`)，彻底杜绝日志文件相互覆盖。
9. **废弃废用的结构体**：彻底清理了早期的 `RenderContext` （曾经用作传递但未实际在 `core/theme.rs` 和 `app.rs` 间使用的遗留包装对象），使数据流传递链路更加纯粹和简单。

### **v0.4.4 — 健壮性与性能优化**

10. **配置项全面持久化**：补充了 `drag_transparent_enabled`（窗口拖拽全透明）等遗漏配置的落盘，并修复了对应事件 `UpdateDragTransparent` 中忘调 `mark_config_dirty` 的问题。
11. **后台线程状态刷新**：修复了系统状态监控线程（sysinfo）在完全无键鼠操作时由于未能触发 `request_repaint` 导致状态栏数据“假死”不更新的问题。
12. **应用退出死锁防御**：`app.rs` 退出钩子 (`on_exit`) 中的等待配置落盘线程机制，补充了 2 秒强制超时打破机制，彻底防止后台异常导致的退出阶段死循环。
13. **高频 Clone 性能优化**：针对 120fps 每帧执行 `self.state.theme.clone()` 可能带来的性能隐患（特别是 `String` 的堆分配开销），重构了内部的生命周期引用传递（如抽出专用的 `add_toast` 函数剥离对 `&mut self` 的不必要独占），确保只有当配置明确发生差异时才进行深度拷贝。
14. **更安全的 IO 降级路径**：将 `current_exe()` 读取异常时的降级方案，从激进的回落到纯粹的 `"."` 父级修改为 `current_dir()`；以及规范化 `cleanup_old_logs` 参数类型避免隐式转换，增强系统级操作安全性。

### **v0.4.5 — 串口终端模块与 UI 风格统一**

15. **串口终端模块 (Terminal)**：实现首个完整业务模块 `modules/terminal.rs`，包含 A/B/C 三栏自适应布局、后台串口线程、双通道通信、RingBuffer 接收缓冲、脏标记 + 渲染缓存 + `show_rows` O(1) 虚拟滚动。
16. **工具按钮配色统一至主题系统**：B/C 区工具按钮（数据筛选、文字高亮、清空显示、单条发送、多条发送）不再使用硬编码颜色，改为引用 `ThemeConfig` 中标题栏按钮配色（`btn_close_bg`、`btn_ai_bg`、`btn_min_bg`、`btn_set_bg`），确保切换主题时所有按钮风格一致且各具辨识度。
17. **状态栏导航按钮风格统一**：主页、串口助手导航按钮采用与业务模块工具按钮一致的风格（圆角、主题色填充、hover 高亮、无边框），各按钮引用不同主题色保持辨识度。
18. **发送区输入框智能垂直对齐**：通过 `desired_rows` 精确匹配实际行数 + `Margin::ZERO` 消除内部偏移，在内容区较矮（≤3 行高）时自动垂直居中光标，拖高后自动切回顶部对齐，兼顾美观与输入直觉。

## **拾壹、已知技术债**

> 已知但暂不阻塞开发，地基稳定后按优先级处理。

| # | 级别 | 问题 | 位置 | 说明 |
|---|------|------|------|------|
| 1 | 🟡 中 | 模块间通信受限 | 架构层 | 模块只能向外壳发事件，不能互相订阅；多模块联动时需扩展广播机制 |
| 2 | 🟡 中 | `app.rs` 职责过重（~1120 行） | `app.rs` | 承担渲染、事件分发、Toast、动画、后台线程等多职责；待拆分子模块 |
| 3 | 🟢 低 | 测试覆盖偏低 | 多处 | 多数文件仅 1 个测试；建议补充配置容错、边界值等场景 |
