# BlueGauge
A lightweight tray tool for easily checking the battery level of your Bluetooth devices.

一款轻便的托盘工具，可轻松查看蓝牙设备的电池电量。

![image](https://raw.githubusercontent.com/iKineticate/BlueGauge/main/screenshots/app.png)

<h3 align="center"> 简体中文 | <a href='./README-en.md'>English</a></h3>

## 功能

- [x] 设置：蓝牙设备电量作为托盘图标    

    - 使用系统字体（默认）：  
        1. 勾选需显示电量设备，打开托盘菜单-`设置`-`打开配置`
        2. 设置相关参数  
        `font_name` = `"系统字体名称，如 Microsoft YaHei UI"`（默认 `Arial`）  
        `font_color` = `"十六进制颜色代码，如 #FFFFFF、#00D26A"`（默认 `FollowSystemTheme`，字体颜色跟随系统主题）  
        `font_size` = `0~255`  （默认 `64`）   
        3. 重新启动 BlueGauge
        4. 其他：图标颜色支持连接配色，在`设置`-`托盘选项`-`设置图标颜色为连接配色`（已连接为绿色，断开连接为红色）

        <div align="center">
            <img src="screenshots/battery.png" style="width=90%; display:block; margin:0 auto 10px;" />
            <div style="display:flex; justify-content:space-between; width:100%; margin:0 auto;">
                <img src="screenshots/connect.png" alt="左下图片" style="width:45%; display:block;">
                <img src="screenshots/disconnect.png" alt="右下图片" style="width:45%; display:block;">
            </div>
        </div>

    - 使用自定义图片  
        1. 在软件目录下创建一个 `assets` 文件夹，
            - 跟随系统主题：在 `assets` 文件夹中，分别创建 `dark` 和 `light` 文件夹，并分别添加 `0.png` 至 `100.png` 照片
            - 不跟随系统主题：在 `assets` 文件夹中添加 `0.png` 至 `100.png` 照片  
        2. 重新启动 BlueGauge

- [x] 设置：开机自启动 

- [x] 设置：蓝牙设备名称别名

    1. 打开托盘菜单-`设置`-`打开配置`   

    2. 在`[device_aliases]`下方添加需要别名的蓝牙设备（注意使用英文引号包裹名称）

        - 例如 `"蓝牙设备名称" = "蓝牙别名"`
        - 例如 `"WH-1000XM6" = "Sony Headphones"`
        - 例如 `"HUAWEI FreeBuds Pro" = "FreeBuds Pro"`
        - 例如 `"OPPO Enco Air3" = "Enco Air3"`

        <div align="center">
            <div style="display:flex; justify-content:space-between; width:100%; margin:0 auto;">
                <img src="screenshots/not_aliases.png" alt="左下图片" style="width:45%; display:block;">
                <img src="screenshots/aliases.png" alt="右下图片" style="width:45%; display:block;">
            </div>
        </div>

- [x] 设置：托盘提示

    - 显示未连接的设备
    - 限制设备名称长度
    - 更改设备电量位置

- [x] 设置：通知

    - 低电量时通知
    - 重新连接时通知
    - 断开连接时通知
    - 添加设备时通知
    - 移除设备时通知

## 下载

1. [Github](https://github.com/iKineticate/BlueGauge/releases/latest)

2. [蓝奏云](https://wwxv.lanzoul.com/b009hchxrc)（密码：6666）

## 已知问题与建议

### 1. 无法获取某些设备电量信息

目前，BlueGauge 可检索低功耗蓝牙设备（BLE）设备和经典蓝牙（Bluetooth Classic）设备的电量，但对于像 **AirPods** 和 **Xbox 控制器** 等使用专有通信协议的设备，可能无法获取电量信息。

- **解决方案：**: 欢迎有能力的开发者贡献代码或提供思路，帮助扩展对这些设备的支持。

### 2. 托盘提示内容不全

托盘提示的字符长度有限，当设备过多和（或）设备名称过长时，提示文本会被截断，导致无法完整显示设备信息。

**建议的解决办法：**

1. **自定义蓝牙设备名称**：通过给蓝牙名称别名缩短其名称长度。

2. **限制设备名称长度**：对设备名称的字符长度进行限制，确保其在托盘通知区域内完整显示。

3. **隐藏未连接的设备**：对于未连接的设备，可以考虑不在托盘通知中显示，从而减少杂乱，避免文本溢出。

### 3. CPU使用率高

可能与某蓝牙设备频繁发送设备电量信息有关

- **解决方案：**: 暂无有效解决办法，可使用最新 `BlueGauge.Debug` 版本排除并断开相关设备连接。

## 其他蓝牙电量软件

 - 支持较多设备：

    - [MagicPods](https://apps.microsoft.com/detail/9P6SKKFKSHKM) (**付费**)   

    - [Bluetooth Battery Monitor](https://www.bluetoothgoodies.com/) (**付费**)   

 - 苹果：[AirPodsDesktop](https://github.com/SpriteOvO/AirPodsDesktop)

 - 华为：[OpenFreebuds](https://github.com/melianmiko/OpenFreebuds)

 - 三星：  

    - [Galaxy Buds](https://apps.microsoft.com/detail/9NHTLWTKFZNB)   

    - [Galaxy Buds Client](https://github.com/timschneeb/GalaxyBudsClient)  

- 罗技: [elem](https://github.com/Fuwn/elem)   

- 赛睿: [Arctis Battery Indicator](https://github.com/aarol/arctis-battery-indicator)   
