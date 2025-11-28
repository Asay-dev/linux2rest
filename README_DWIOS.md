# linux2rest - DwiOS Fork

这是 [patrickelectric/linux2rest](https://github.com/patrickelectric/linux2rest) 的 fork 版本，
添加了通用平台支持，使其能在非 Raspberry Pi 的 Linux 系统上正常运行。

## 主要改进

### 1. 通用平台支持 (GenericPlatform)

原版 linux2rest 只支持 Raspberry Pi，在其他平台上会返回 `UnknownModel` 错误。
本 fork 添加了 `GenericPlatform` 结构，可以在任何 Linux 系统上正常工作：

- **ARM 开发板**: Radxa Rock, NVIDIA Jetson, Orange Pi, Khadas VIM 等
- **x86/x64 系统**: 任何 Linux PC 或服务器
- **Raspberry Pi**: 继续完整支持所有树莓派型号

### 2. 平台信息来源

| 信息 | ARM 设备 | x86 设备 |
|------|----------|----------|
| 型号 | `/proc/device-tree/model` | `/sys/devices/virtual/dmi/id/product_name` |
| 架构 | `uname -m` | `uname -m` |
| CPU | `lscpu` 或 `/proc/cpuinfo` | `lscpu` |
| 内核 | `uname -r` | `uname -r` |
| 系统 | `/etc/os-release` | `/etc/os-release` |

### 3. API 响应示例

在 Radxa Rock 3C 上:
```json
{
  "generic": {
    "model": "Radxa ROCK 3 Model C",
    "arch": "aarch64",
    "cpu_name": "Cortex-A55",
    "kernel": "5.10.160-rockchip",
    "os_name": "Ubuntu 22.04.3 LTS"
  },
  "raspberry": null
}
```

在 Raspberry Pi 4 上 (启用 raspberry 特性):
```json
{
  "generic": {
    "model": "Raspberry Pi 4 Model B Rev 1.4",
    "arch": "aarch64",
    "cpu_name": "Cortex-A72",
    "kernel": "6.1.21-v8+",
    "os_name": "Raspberry Pi OS"
  },
  "raspberry": {
    "model": "Raspberry Pi 4 Model B",
    "soc": "BCM2711",
    "serial": "100000001234abcd",
    "events": {...}
  }
}
```

## 编译说明

### 通用编译 (不支持 Raspberry Pi 特定功能)
```bash
cargo build --release
```

### 包含 Raspberry Pi 支持
```bash
cargo build --release --features raspberry
```

## 使用方法

1. 将此仓库 fork 到您的 GitHub 账户
2. 创建 Release (例如 v0.9.0)
3. 编译并上传二进制文件
4. 更新 DwiOS 的 `core/tools/linux2rest/bootstrap.sh` 指向您的仓库

## 交叉编译

为 ARM64 编译:
```bash
# 安装交叉编译工具链
sudo apt install gcc-aarch64-linux-gnu

# 添加 target
rustup target add aarch64-unknown-linux-gnu

# 编译
cargo build --release --target aarch64-unknown-linux-gnu
```

为 ARMv7 编译:
```bash
sudo apt install gcc-arm-linux-gnueabihf
rustup target add armv7-unknown-linux-gnueabihf
cargo build --release --target armv7-unknown-linux-gnueabihf
```

## 版本历史

- **v0.9.0**: 添加通用平台支持
- **v0.8.1**: 上游原版 (仅 Raspberry Pi)

## 致谢

原项目作者: Patrick José Pereira <patrickelectric@gmail.com>
