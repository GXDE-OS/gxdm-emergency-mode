# GXDE Display Manager (Rescue Mode)
## Introduction
This program helps you to login your GUI sessions when your GXDE Display Manager does not work.

This program does not contain any graphical user interface, all operations are happening under TTY VT8.

## Building
### Dependencies
Please have `rustc`, `cargo`, `libpam0g-dev`, `libsystemd-dev` and `libclang-dev` ready.

### Building Manually
```shell
$ cargo build --release
```

### Packaging
#### Debian
```shell
$ chmod a+x ./build-deb
$ ./build-deb -d  # Install depencies, build and generate .deb package. Next time you may run ./build-deb to skip the dependency check.
$ ./build-deb -c  # This command cleans up the repo. Note that the artifacts will be cleared.
```

## Usage
### Starting Manually
> **NOTE**: For some old version, you may need to switch to VT8 manually. Please run under TTY.

```shell
$ sudo systemctl stop <the display manager you're using>.service     # e.g. sudo systemctl stop gxdm.service
$ sudo systemctl disable <the display manager you're using>.service  # e.g. sudo systemctl disable gxdm.service
$ sudo systemctl enable gxdm-rescue.service
$ sudo systemctl start gxdm-rescue.service
```

### Restoring to Your Preferred Display Manager
> **Note**: Please run under TTY.

```shell
$ sudo systemctl stop gxdm-rescue.service
$ sudo systemctl disable gxdm-rescue.service
$ sudo systemctl enable <the display manager you're using>.service  # e.g. sudo systemctl enable gxdm.service
$ sudo systemctl start <the display manager you're using>.service   # e.g. sudo systemctl start gxdm.service
```

## License
(C) 2026 CharOfString & GXDE Maintainers.

GXDE Display Manager (Rescue Mode) is licensed under GNU GENERAL PUBLIC LICENSE Version 3. You may find a copy of the license [here](./LICENSE).
