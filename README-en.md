# BlueGauge
A lightweight tray tool for easily checking the battery level of your Bluetooth devices.

![image](screenshots/app.png)

<h3 align="center"> <a href='./README.md'>简体中文</a> | English</h3>

## Function

- [x] Setting：Bluetooth battery level as tray icon  

    - Use number icon (default):  
        1. check the device that needs to display the battery    
        2. set font: open tray menu -- `Settings` -- `Open Config`   
        `font_name` = `"System Font Nmae, e.g. Microsoft YaHei UI"`  
        `font_color` = `"Hex color code，e.g. #FFFFFF、#00D26A"` (Default `"FollowSystemTheme"`)  
        `font_size` = `0~255` (Default `64`)   
        3. restart BlueGauge
        4. others: the icon color supports connection color matching, set the icon color to the connection color in `Settings`-`Tray Options` (connected as green, disconnected as red)

        <div align="center">
            <img src="screenshots/battery.png" style="width=90%; display:block; margin:0 auto 10px;" />
            <div style="display:flex; justify-content:space-between; width:100%; margin:0 auto;">
                <img src="screenshots/connect.png" alt="左下图片" style="width:45%; display:block;">
                <img src="screenshots/disconnect.png" alt="右下图片" style="width:45%; display:block;">
            </div>
        </div>

    - Use ring icon:
        1. check the device that needs to display the battery    
        2. open tray menu - `Settings` - `Tray Options` - `Icon Style` - `Ring Icon`   
        3. set color, open tray menu -- `Settings` -- `Open Config`  
        `highlight_color` = `"Hex color code，e.g. #00D26A"`（Default green）    
        `background_color` = `"Hex color code，e.g.#A7A19B"`（Default gray）   
        4. restart BlueGauge 
        5. Highlight color turns red when the device battery is low   

        <div align="center">
            <div style="display:flex; justify-content:space-between; width:100%; margin:0 auto;">
                <img src="screenshots/ring.png" alt="左下图片" style="width:45%; display:block;">
                <img src="screenshots/ring_low_battery.png" alt="右下图片" style="width:45%; display:block;">
            </div>
        </div>

    - Use custom icon  
        1. create an `assets` folder in the BlueGauge directory
            - Default：add `0.png` to `100.png`   
            - Follow system theme：In the `assets` folder, create the `dark` and `light` folders respectively, and add `0.png` to `100.png` photos respectively
        2. restart BlueGauge  


- [x] Setting：Auto start
- [x] Setting：Bluetooth device name aliases

    1. open tray menu -- `Settings` -- `Open Config`   

    2. Add the required Bluetooth device alias under `[device_aliases]` (note that you use quotation marks to wrap the name)

        - e.g. `"Bluetooth device name" = "Bluetooth alias"`
        - e.g. `"WH-1000XM6" = "Sony Headphones"`
        - e.g. `"Surface Pen" = "Pen"`
        - e.g. `"HUAWEI FreeBuds Pro" = "FreeBuds Pro"`
        - e.g. `"OPPO Enco Air3" = "Enco Air3"`

        <div align="center">
            <div style="display:flex; justify-content:space-between; width:100%; margin:0 auto;">
                <img src="screenshots/not_aliases.png" alt="左下图片" style="width:45%; display:block;">
                <img src="screenshots/aliases.png" alt="右下图片" style="width:45%; display:block;">
            </div>
        </div>

- [x] Setting: tooltip

    - Shows unconnected devices
    - Truncate devices Name
    - Changing the device power location

- [x] Setting: notice

    - Low battery notice
    - Notification when reconnecting the device
    - Notification when disconnecting the device
    - Notification when adding a new device
    - Notification when moving a new device

## Download: 


[Github](https://github.com/iKineticate/BlueGauge/releases/latest) ( Please download the x86_64 version by default, and download the ARM version for special systems Windows on ARM. )

## Known Issues & Suggested Solutions

### 1. Currently, BlueGauge successfully retrieves battery levels from Bluetooth low-energy devices and Bluetooth Classic devices. However, we are unable to fetch the battery status from devices like AirPods and Xbox controllers, which operate on proprietary communication protocols.

**Solution:**

Welcome contributions from developers who can help us extend support for these devices.


### 2. The character length of tray tooltip is currently limited. When the tooltip text exceeds this limit, it gets truncated, which can result in incomplete device names being displayed. This can cause confusion for users, especially when multiple devices are connected.

**Solution:**

1. **Custom Bluetooth Name**：Shorten the length of the name by giving the Bluetooth name alias.

2. **Limit Device Name Length**: Implement a character limit for device names that ensures they fit within the available space of the tray notification. This may require shortening longer names to prevent truncation.

3. **Hide Disconnected Devices**: Consider not displaying disconnected devices in the tray notifications. This approach would reduce clutter and ensure that only relevant information is shown, thereby preventing text overflow.

### 3. High CPU usage rate

It may be related to the frequent sending of device battery information in a Bluetooth device

**Solution:**:   

There is no effective solution yet. You can use the latest `BlueGauge.Debug` version to exclude and disconnect related devices.   

### 4. How to display the power of multiple devices on the pallet?

- **Solution:**: Create another folder, copy `BlueGauge.exe` to the folder, then rename `BlueGauge.exe` to another name, and finally open and set the `Launch at Startup`

### 5. The tray battery icon is cut off by a small part

- **Solution::**: Open the tray menu - `Settings`-`Open Config`, and then set the font size in `font_size` under `[tray.icon.font]` to set a font that is one size smaller than the original one.

### 6. Connection indicator in tray prompt has no color

Connection indicator only supports displaying colors in Windows 11

- **Solution::**: Open the tray menu - `Settings`-`Open Config`, and then set the font size in `font_size` under `[tray.icon.font]` to set a font that is one size smaller than the original one.

## Other Bluetooth battery display software

 - Supports more devices：

    - [MagicPods](https://apps.microsoft.com/detail/9P6SKKFKSHKM) (**Purchase**)   

    - [Bluetooth Battery Monitor](https://www.bluetoothgoodies.com/) (**Purchase**)   

 - Apple: [AirPodsDesktop](https://github.com/SpriteOvO/AirPodsDesktop)

 - Huawei: [OpenFreebuds](https://github.com/melianmiko/OpenFreebuds)

 - Samsung:

    - [Galaxy Buds](https://apps.microsoft.com/detail/9NHTLWTKFZNB)

    - [Galaxy Buds Client](https://github.com/timschneeb/GalaxyBudsClient)  

- Logitech: [elem](https://github.com/Fuwn/elem)   

- SteelSeries Arctis: [Arctis Battery Indicator](https://github.com/aarol/arctis-battery-indicator)   
