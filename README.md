# SolidCraft for HarmonyOS（鸿蒙）

把 [SolidCraft](https://github.com/tangency-labs/solidcraft)（Rust + egui 的 CAD/B-rep 内核）
移植到 HarmonyOS（DevEco Studio 工程），产出可在真机上安装的 `.hap`。

- **移植对象**：`tangency-labs/solidcraft`
- **移植方法**：参照 `photocraft-hmos` 的成熟经验（三层架构 + 双 ABI 产物 + 离线签名），
  **不 patch 上游**——SolidCraft 的 `SolveApp` 已原生暴露宿主缝。
- **状态**：已在 HUAWEI MateBook Fold / API 26 / Maleoon 916 真机上安装运行通过，
  界面完整上屏、输入可用、`.step` 加载正常。

> 完整、如实区分「已验证 / 未验证」的说明见
> [移植报告.md](./移植报告.md)。

## 架构（三层）

```
ArkTS (UIAbility + XComponent)
   │  N-API (.d.ts: init / input / reply / open / drop / active / close / command)
   ▼
C++ N-API 桥 (entry/src/main/cpp/bridge.cpp)  → 链接 libsolidcraft_hmos.a
   ▼
Rust 适配层 (native/rust, staticlib "solidcraft_hmos", path 依赖上游 crate)
   ▼
solidcraft-ui-egui (SolveApp) + solidcraft-engine + egui / egui-wgpu / wgpu(GLES)
```

渲染走 wgpu **GLES** 后端，直接在鸿蒙 `NativeWindow` 上绘制 egui（不经 eframe / winit）。

## 目录

| 路径 | 说明 |
| --- | --- |
| `entry/` | DevEco 应用模块：ArkTS（`src/main/ets`）、C++ N-API 桥（`src/main/cpp`）、资源 |
| `native/rust/` | 独立 Rust 适配层 workspace，`path` 依赖上游 crate，产出 `libsolidcraft_hmos.a` |
| `third_party/` | vendored `wgpu-hal-30.0.1`（含 GLES uniform-block 修补） |
| `scripts/` | 本地开发循环 `run_on_device.sh`、签名辅助脚本等 |
| `docs/` `移植报告.md` | 详细移植报告 |

## 本地构建与安装

```bash
# 需要 DevEco Studio SDK（native/hvigor/node）已就位
bash scripts/run_on_device.sh <设备号>
# 流程：cargo build(arm64-ohos) → assembleHap(DevEco 签名) → hdc install → 启动
```

> **签名**：调试 profile 由 DevEco 自动签名（AGC 账号）生成到 `~/.ohos/config/`；
> 工程内 `signing/` 目录含本地自签 CA 材料，已在 `.gitignore` 中排除，**不入库**。

## 说明

- 上游 `crates/*` 零改动；所有鸿蒙逻辑均在独立适配层。
- SMLib（NVIDIA B-rep C++ 内核）在 OHOS 目标下经 DevEco `clang++` 交叉编译后静态链入 `libsolidcraft.so`。
