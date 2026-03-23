# Files Rusted Selection 边界整理设计

## 背景

`Files Rusted` 目前已经具备可运行的文件管理器 MVP，并且刚刚完成首版桌面式矩形框选。当前 `src/browser.rs` 已经同时承载目录读取、导航、选择模型、拖框选择、文件操作、剪贴状态和窗口同步等多类职责。

继续直接在 `browser.rs` 上堆功能会让后续维护越来越困难，尤其是选择相关逻辑已经自然长成一个独立子系统。相比马上继续堆自动滚动、拖拽增强或更大功能，这一轮更合适的动作是先把 selection / drag selection 的边界整理清楚。

## 目标

在不推翻现有架构和交互语义的前提下，完成一次最小拆分版 selection 边界整理：

- 把 `src/browser.rs` 中 selection / drag selection 相关逻辑收口为更清晰的小模块。
- 保持 `BrowserState` 仍然是对外总入口。
- 保持 `main.rs` 继续只做 Slint 回调 wiring。
- 保持 `ui/app-window.slint` 继续只负责输入和显示。
- 不改变现有点击、键盘、多选、矩形框选语义。

## 非目标

本轮明确不做：

- 不拆 navigation state
- 不拆 file operations service
- 不新增自动滚动
- 不新增地址栏、标签页、双栏等大功能
- 不顺手重写 UI 结构
- 不做大规模测试框架调整

## 范围

### 这轮包含

- 新建 selection 相关子模块
- 新建 drag selection 相关子模块
- 把基础选择状态与选择操作从 `browser.rs` 中收口
- 把拖框几何、命中、会话推导逻辑从 `browser.rs` 中收口
- 对现有测试做最小迁移，保证测试继续覆盖该子系统

### 这轮不包含

- 不处理导航和历史管理的进一步拆分
- 不处理复制/剪切/粘贴、重命名、删除等文件操作的进一步拆分
- 不借机统一整个代码库命名风格
- 不新增与当前目标无关的抽象层

## 模块边界

### `src/browser.rs`

继续保留：

- `BrowserState` 作为对外 façade / coordinator
- 目录读取与刷新入口
- 导航入口
- 文件操作入口
- 与 Slint 窗口属性同步
- 将 UI 事件转发给 selection 子模块

`browser.rs` 的目标不再是承载 selection 细节，而是作为总编排层存在。

### `src/browser/selection.rs`

负责基础选择状态与选择语义，例如：

- selected paths
- primary / focus path
- anchor path
- 单选
- toggle 选中
- 范围选择
- 清空选择
- select all
- 选择集合去重与归一化
- 将拖框结果落入统一 selection model

它的职责是定义“当前选中了什么、焦点在哪里、锚点在哪里，以及选择如何变化”。

### `src/browser/drag_selection.rs`

负责拖框相关纯逻辑，例如：

- `DragPoint`
- `DragRect`
- `VisibleItemLayout`
- `DragSelectionSession`
- 拖框阈值判断
- 矩形命中
- `Ctrl` 拖框与基线选择的合并/切换推导
- 从拖框结果推导 primary / anchor

它的职责是定义“拖框命中了什么，以及这次拖框应该产生什么选择结果”，而不是持有整个浏览器状态。

## 迁移策略

本轮采用“搬运 + 收口 + 轻接口整理”的策略，而不是激进重构。

### 第一步：建立模块骨架

在 `src/browser/` 下建立最小模块结构：

- `selection.rs`
- `drag_selection.rs`

### 第二步：先搬纯逻辑

优先把下列相对纯净的逻辑搬出 `browser.rs`：

- 拖框几何结构
- 命中判断
- baseline selection -> result 推导
- selection 集合去重、toggle、range 等基础逻辑

### 第三步：保留稳定入口

`BrowserState` 对外方法名尽量保持稳定，避免把重构波及到：

- `main.rs` 的 callback wiring
- `app-window.slint` 的行为语义

也就是说，外层入口尽量不变，内部实现改为委托给子模块。

### 第四步：最小测试迁移

测试不做大整理，只做最小迁移：

- 保留当前源文件邻近测试风格
- 仅把必须跟着私有类型移动的测试一起迁移
- 保持现有测试覆盖 selection / drag selection 的关键语义

## 关键约束

### 架构约束

- Rust 继续负责状态与业务语义
- Slint 继续负责输入与显示
- `main.rs` 继续做轻 wiring
- 不把业务逻辑重新堆回 UI 层

### 复杂度约束

- 不新增 trait 层或过度抽象
- 不为了模块化而做无收益的改名风暴
- 不把这轮“最小拆分”演变成“全面重构”

## 兼容性要求

重构后以下行为必须保持一致：

- 单击条目选中
- 双击目录进入 / 双击文件打开
- 空白处左键清空选择
- `Ctrl+方向键` 只移动焦点
- `Shift+方向键` 按 anchor 扩选
- `Space` / `Ctrl+Space` 语义不变
- 当前矩形框选行为继续可用
- `Ctrl` 拖框保留未命中的基线选择
- 无命中拖框清空选择
- `Ctrl + 空白单击` 不误清空选择

## 测试策略

### 单元测试

保持并验证现有选择相关测试，包括：

- 无修饰键拖框命中单项 / 多项
- `Ctrl` 拖框与基线选择合并/切换
- 无命中拖框清空选择
- 拖框完成后 focus / anchor 更新
- 普通空白点击清空选择
- `Ctrl + 空白单击` 保留选择

### 构建与运行验证

至少执行：

1. `cargo test`
2. `cargo build`
3. `xvfb-run -a ./target/debug/files-rusted`

如果出现失败，要明确区分：

- 编译失败
- 运行时失败
- headless 环境限制

## 完成标准

本轮完成的标志是：

- `browser.rs` 中 selection 相关职责显著收口
- selection / drag selection 具备清晰模块边界
- `BrowserState` 仍然是总入口
- 现有矩形框选与选择语义不变
- `cargo test`、`cargo build`、`xvfb-run` 验证通过

## 实施结论

这一轮的价值不是增加新按钮，而是为后续继续推进桌面式选择交互、自动滚动、选择模型增强以及更大状态层拆分打基础。

它应该是一次聚焦、最小、可验证的结构整理，而不是新一轮架构重做。