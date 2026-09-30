# gxdm-emergency-mode

A terminal-based emergency display manager for GXDE. Login uses PAM and
**systemd-logind**, not seatd. The existing yellow focus highlight is retained;
only the selected, borderless Login action has the `❃` prefix.

## Build

On Debian/GXDE, install native build dependencies, then build with Rust:

```sh
sudo apt install libpam0g-dev libsystemd-dev libclang-dev
cargo build --release
```

Runtime dependencies: systemd-logind, libpam-systemd, libpam-modules,
libpam-runtime, and dbus-user-session. X11 sessions also need `xinit`, `xauth`
and `xserver-xorg-core`. Install the desktop/compositor separately.

## Install and run on a dedicated VT

**Test in a VM first.** This is privileged display-manager code; unit tests do
not replace an end-to-end PAM/logind/graphics test. The supplied service uses
**tty8 on seat0**. Check that no other display manager is using that VT before
starting it. Do not replace your working display manager yet.

Build and install the Debian package. It installs the binary to `/usr/sbin`,
the PAM policy to `/etc/pam.d/gxdm-rescue` and the systemd unit, but does not
enable or start the service. `cargo` needs the crates in its local cache or
network access.

```sh
sudo apt install debhelper cargo rustc
dpkg-buildpackage -us -uc -b
sudo apt install ../gxdm-emergency-mode_0.1.0-gxde1_amd64.deb
sudo systemctl start gxdm-rescue.service
sudo chvt 8
```

Without packaging, install the same files by hand:

```sh
sudo install -o root -g root -m 0755 target/release/gxdm-emergency-mode /usr/sbin/
sudo install -o root -g root -m 0644 data/gxdm-rescue.pam /etc/pam.d/gxdm-rescue
sudo install -o root -g root -m 0644 data/gxdm-rescue.service /etc/systemd/system/
sudo systemctl daemon-reload
```

`systemctl start` runs the service for this boot only. It does not enable it at
boot. The service conflicts with `getty@tty8.service`, not your current display
manager. Stop it from another VT or an administrative terminal with:

```sh
sudo systemctl stop gxdm-rescue.service
journalctl -u gxdm-rescue.service -b
```

Do not install the binary setuid. Starting it from a normal terminal is useful
for UI preview, but actual login requires a root system service with a Linux
controlling VT and **no existing logind session**. In particular, `sudo` inside
an existing login/SSH/desktop session is not a supported launch method. Do not
add `PAMName=` to the systemd unit: PAM belongs to the per-login worker.

Use Up/Down or Tab to select a field, Left/Right to choose a desktop, and Enter
on Login to authenticate. A failed login clears the submitted password. When
the desktop exits, the greeter returns. Ctrl-C exits the greeter.

## Login lifecycle

1. Transfer credentials to a fresh worker through a private anonymous pipe,
   never command arguments, environment variables, or a password file.
2. Authenticate with PAM and check account policy. Expired/locked accounts and
   empty passwords are rejected; graphical root login is disabled.
3. Open the PAM session with the VT, seat and X11/Wayland metadata. The installed
   PAM policy **requires** `pam_systemd.so`. Verify the worker's logind session
   ID, UID, seat, VT, active state and runtime directory before launching anything.
4. Start the desktop with the user's primary/supplementary groups and UID,
   home directory, PAM environment and user bus. The greeter's environment is
   not inherited. `LIBSEAT_BACKEND=logind` is set for libseat-based compositors.
5. For X11, use `startx`/`xinit` to manage Xauthority and Xorg on the same VT,
   with TCP disabled. For Wayland, launch the selected compositor directly.
6. Wait for logout, close PAM while still privileged, and ask logind to terminate
   that session's remaining processes. Restore terminal ownership, attributes
   and foreground control before showing the greeter again. Termination signals
   are forwarded, with bounded waits before forcibly stopping the worker.

The password buffers owned by the request/conversation are cleared after use;
core dumps are disabled. PAM modules manage their own credential copies.
Only root-owned, non-group/world-writable system session files and parent paths
are accepted, from `/usr/share/xsessions` and `/usr/share/wayland-sessions`.
`Exec` is parsed into arguments, not evaluated as a shell command.

### Current limits

- Local seat0/Linux VT operation only; no remote or multi-seat login.
- Password-only conversation. Additional hidden challenges (MFA) fail closed;
  password changes must be performed through another login tool.
- Session `Exec` supports double-quoted arguments and literal `%%`, but rejects
  other desktop field codes. Relative executable paths containing `/` are
  rejected. The desktop launcher must remain in the foreground until logout.
- The PAM policy is Debian/GXDE-specific. Sites using SELinux, custom PAM session
  modules or other distributions must review/adapt it before deployment.
- User-managed systemd services outside the login session scope are not stopped.
- This version does not run `/etc/X11/Xsession` or shell profile hooks around the
  selected command. Desktops requiring such setup need a suitable session wrapper.

Architecture references: [pam_systemd](https://www.freedesktop.org/software/systemd/man/latest/pam_systemd.html),
[PAM context/session API](https://docs.rs/pam-client/latest/pam_client/struct.Context.html),
[desktop Exec syntax](https://specifications.freedesktop.org/desktop-entry-spec/latest/exec-variables.html),
[startx](https://xorg.freedesktop.org/releases/X11R6.8.2/doc/startx.1.html).

## Checks

```sh
./update-header
./format-code
./lint-code
cargo test
./update-header --check
./format-code --check
```

Tests cover request framing/limits, password handling, desktop argument parsing,
launch command construction, process supervision and UI behavior. They do not
authenticate real users or start graphical sessions.

For VM acceptance testing, check wrong passwords and locked/expired accounts,
then valid X11 and Wayland logins. Inside each desktop check `id`, the XDG
variables and `loginctl session-status`; confirm the selected UID, tty8, seat0,
active state, runtime directory and lack of seatd usage. Log out, repeat with a
second user, and check that the previous session is gone and the greeter still
works. Also stop the service during login and during a running desktop, and test
missing pam_systemd/startx, a failed desktop executable and a small terminal.
