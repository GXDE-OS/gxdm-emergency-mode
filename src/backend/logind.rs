// Copyright (C) 2026 CharOfString <root@charofstring.cc>
//
//
// This software is free software: you can redistribute it and/or modify it under the terms of the
// GNU General Public License as published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This software is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with this software. If
// not, see <https://www.gnu.org/licenses/>.

//! Verify the session created by pam_systemd using libsystemd.

use super::desktop::invalid;
use std::{
    ffi::{CStr, CString},
    io, ptr,
};

#[link(name = "systemd")]
unsafe extern "C" {
    fn sd_pid_get_session(pid: libc::pid_t, session: *mut *mut libc::c_char) -> libc::c_int;
    fn sd_session_get_uid(session: *const libc::c_char, uid: *mut libc::uid_t) -> libc::c_int;
    fn sd_session_get_vt(session: *const libc::c_char, vt: *mut libc::c_uint) -> libc::c_int;
    fn sd_session_get_seat(
        session: *const libc::c_char,
        seat: *mut *mut libc::c_char,
    ) -> libc::c_int;
    fn sd_session_is_active(session: *const libc::c_char) -> libc::c_int;
}

pub fn current_session() -> io::Result<Option<String>> {
    let mut raw = ptr::null_mut();
    // libsystemd allocates the returned string with malloc.
    let result = unsafe { sd_pid_get_session(0, &mut raw) };
    if result == -libc::ENODATA || result == -libc::ENOENT {
        return Ok(None);
    }

    if result < 0 {
        return Err(io::Error::from_raw_os_error(-result));
    }

    let session = unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    unsafe {
        libc::free(raw.cast());
    }

    Ok(Some(session))
}

pub fn verify(id: &str, uid: u32, vt: u32) -> io::Result<()> {
    if current_session()?.as_deref() != Some(id) {
        return Err(invalid(
            "pam_systemd did not create a logind session for the worker",
        ));
    }

    let id = CString::new(id)?;
    let mut actual_uid = 0;
    let mut actual_vt = 0;

    // All output pointers refer to valid local storage.
    unsafe {
        if sd_session_get_uid(id.as_ptr(), &mut actual_uid) < 0
            || sd_session_get_vt(id.as_ptr(), &mut actual_vt) < 0
            || sd_session_is_active(id.as_ptr()) <= 0
            || actual_uid != uid
            || actual_vt != vt
        {
            return Err(invalid(
                "logind session user, VT or active state does not match",
            ));
        }
    }

    let mut seat = ptr::null_mut();
    let result = unsafe { sd_session_get_seat(id.as_ptr(), &mut seat) };
    if result < 0 {
        return Err(io::Error::from_raw_os_error(-result));
    }

    let is_seat0 = unsafe { CStr::from_ptr(seat) } == c"seat0";
    unsafe {
        libc::free(seat.cast());
    }

    if !is_seat0 {
        return Err(invalid("Session is not on seat0!"));
    }
    Ok(())
}
