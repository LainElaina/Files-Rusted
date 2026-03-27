# Windows 本地构建运行指南

这份文档用于指导你在 **Windows** 上把 `Files Rusted` 构建出来并实际打开看效果。

## 1. 你要达到的结果

最终你应该能在项目根目录执行：

```powershell
cargo run
```

或者先构建再直接运行：

```powershell
cargo build
.\target\debug\files-rusted.exe
```

如果一切正常，你会看到 `Files Rusted` 的窗口启动出来，可以直接在 Windows 桌面环境里看当前 UI 和交互效果。

---

## 2. 当前项目的技术前提

这个项目当前使用的是：

- Rust 2021
- Slint 1.15.1
- `build.rs` 会编译 `ui/app-window.slint`

也就是说，Windows 上要能成功构建，至少需要：

1. Rust 工具链
2. **MSVC** 目标工具链
3. Visual Studio C++ 构建工具

> 建议直接走 **MSVC** 路线，不要用 GNU 工具链折腾。

---

## 3. 建议的 Windows 环境

建议你使用：

- Windows 10 / Windows 11
- PowerShell 或 Windows Terminal
- Visual Studio 2022 Build Tools（或 Visual Studio Community 2022）
- Rustup + stable toolchain

---

## 4. 先安装依赖

### 4.1 安装 Visual Studio C++ 构建工具

如果你还没装，安装下面任意一种：

- **Visual Studio 2022 Community**
- **Build Tools for Visual Studio 2022**

安装时至少勾选：

- **Desktop development with C++**

如果安装器里有可选项，建议一并带上：

- MSVC v143 toolset
- Windows 10/11 SDK
- CMake tools for Windows

装完以后，**重新打开终端**。

---

### 4.2 安装 Rustup

如果你还没装 Rust，打开 PowerShell 执行：

```powershell
winget install Rustlang.Rustup
```

装完以后，重新开一个新的 PowerShell。

检查是否成功：

```powershell
rustup -V
rustc -V
cargo -V
```

---

### 4.3 切到 MSVC stable 工具链

执行：

```powershell
rustup toolchain install stable-x86_64-pc-windows-msvc
rustup default stable-x86_64-pc-windows-msvc
```

确认当前工具链：

```powershell
rustup show
```

你应该能看到类似：

```text
stable-x86_64-pc-windows-msvc (default)
```

如果你只想补 target，也可以执行：

```powershell
rustup target add x86_64-pc-windows-msvc
```

---

## 5. 获取项目代码

如果你已经有这份仓库，直接进入项目目录即可。

假设项目目录是：

```text
D:\code\Files Rusted
```

那就在 PowerShell 里执行：

```powershell
cd "D:\code\Files Rusted"
```

然后确认当前目录里能看到这些文件：

- `Cargo.toml`
- `build.rs`
- `src\`
- `ui\app-window.slint`

---

## 6. 第一次构建前建议先跑测试

先执行：

```powershell
cargo test
```

如果通过，再执行：

```powershell
cargo build
```

如果你只是想尽快看效果，也可以直接：

```powershell
cargo run
```

但我更建议顺序是：

1. `cargo test`
2. `cargo build`
3. `cargo run`

---

## 7. 直接运行看效果

### 方案 A：一条命令直接启动

```powershell
cargo run
```

### 方案 B：先构建，再运行 exe

```powershell
cargo build
.\target\debug\files-rusted.exe
```

如果你想构建 release 版本：

```powershell
cargo build --release
.\target\release\files-rusted.exe
```

---

## 8. 运行后你应该重点看什么

当前项目已经不是空壳，可以重点看这些：

- 目录浏览是否正常
- `Home / Up / Refresh`
- `Back / Forward`
- 面包屑导航
- 排序 / 筛选
- 单选 / 多选 / 范围选择
- 空白区域点击清空选择
- 矩形框选
- 拖框自动滚动
- 右键菜单
- `New File / New Folder / Rename / Delete`
- `Copy / Cut / Paste`

因为 Windows 是真实桌面环境，**非常适合直接看当前交互手感**。

---

## 9. 最常见的命令清单

### 进入项目目录

```powershell
cd "D:\code\Files Rusted"
```

### 运行测试

```powershell
cargo test
```

### 调试构建

```powershell
cargo build
```

### 直接启动

```powershell
cargo run
```

### 运行调试版 exe

```powershell
.\target\debug\files-rusted.exe
```

### 构建发布版

```powershell
cargo build --release
```

### 运行发布版 exe

```powershell
.\target\release\files-rusted.exe
```

---

## 10. 常见报错与处理办法

### 10.1 `link.exe` 找不到 / linker 错误

常见现象：

- `link.exe not found`
- `linker 'link.exe' not found`
- 一堆 MSVC 相关链接错误

这通常表示：

- 没装 Visual Studio C++ 构建工具
- 或者装了但终端没重开

处理办法：

1. 安装 **Desktop development with C++**
2. 重启终端
3. 再跑：

```powershell
cargo build
```

---

### 10.2 `cargo` / `rustc` 命令不存在

说明 Rustup 没装好，或者 PATH 还没刷新。

处理办法：

1. 重新打开 PowerShell
2. 再执行：

```powershell
rustup -V
cargo -V
```

如果还是不行，重新安装：

```powershell
winget install Rustlang.Rustup
```

---

### 10.3 target 不对，走成 GNU 了

建议不要混用 GNU。

执行：

```powershell
rustup default stable-x86_64-pc-windows-msvc
rustup show
```

确认默认工具链是：

```text
stable-x86_64-pc-windows-msvc
```

---

### 10.4 Slint 编译失败

当前项目在 `build.rs` 里会编译：

```text
ui/app-window.slint
```

如果这里报错，一般有两类原因：

1. `ui/app-window.slint` 本身有语法/属性问题
2. Rust 侧和 Slint 侧接口不一致

先做：

```powershell
cargo build
```

然后把完整错误输出保存下来，不要只截最后一行。

---

### 10.5 程序能编过，但启动后闪退

先不要猜。

在 PowerShell 里直接运行：

```powershell
cargo run
```

或者：

```powershell
.\target\debug\files-rusted.exe
```

这样错误会直接打印在终端里，比双击 exe 更容易看清楚。

---

## 11. 推荐的实际操作顺序

如果你只是想最稳地把它跑起来，建议按这个顺序：

### 第一步：确认工具链

```powershell
rustup show
rustc -V
cargo -V
```

### 第二步：进项目目录

```powershell
cd "D:\code\Files Rusted"
```

### 第三步：先测

```powershell
cargo test
```

### 第四步：再构建

```powershell
cargo build
```

### 第五步：启动看效果

```powershell
cargo run
```

---

## 12. 如果你只想最快看到界面

最短路径就是：

```powershell
cd "D:\code\Files Rusted"
rustup default stable-x86_64-pc-windows-msvc
cargo run
```

前提是你已经装好了：

- Rustup
- Visual Studio C++ Build Tools

---

## 13. 我建议你看完效果后怎么反馈

如果你在 Windows 真机上成功跑起来了，下一步最有价值的是直接记这几类反馈：

1. 能不能正常启动
2. 窗口显示是否正常
3. 列表、选择、右键菜单是否正常
4. 拖框自动滚动在真实桌面环境里的手感如何
5. 有没有只在 Windows 上出现的路径、打开方式、输入事件问题

如果你愿意，后面可以直接把你的终端输出或报错原文贴给我，我可以继续按报错一步步带你排。
