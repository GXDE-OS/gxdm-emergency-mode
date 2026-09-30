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

//! Password-only PAM conversation. Do not reuse the password for extra challenges.

use pam_client::{ConversationHandler, ErrorCode};
use std::ffi::{CStr, CString};
use zeroize::{Zeroize, Zeroizing};

pub struct Conversation {
    pub username: String,
    pub password: Zeroizing<String>,
    answered: bool,
}

impl Conversation {
    pub fn new(username: String, password: Zeroizing<String>) -> Self {
        Self {
            username,
            password,
            answered: false,
        }
    }
}

impl ConversationHandler for Conversation {
    fn prompt_echo_on(&mut self, _: &CStr) -> Result<CString, ErrorCode> {
        CString::new(self.username.as_str()).map_err(|_| ErrorCode::CONV_ERR)
    }

    fn prompt_echo_off(&mut self, _: &CStr) -> Result<CString, ErrorCode> {
        if self.answered {
            return Err(ErrorCode::CONV_ERR);
        }
        self.answered = true;
        let reply = CString::new(self.password.as_bytes()).map_err(|_| ErrorCode::CONV_ERR);
        self.password.zeroize();
        reply
    }

    // PAM messages may contain account details. Do not put them in logs.
    fn text_info(&mut self, _: &CStr) {}
    fn error_msg(&mut self, _: &CStr) {}
    fn radio_prompt(&mut self, _: &CStr) -> Result<bool, ErrorCode> {
        Err(ErrorCode::CONV_ERR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_reuses_password_for_second_challenge() {
        let mut conv = Conversation::new("user".into(), Zeroizing::new("secret".into()));
        assert_eq!(
            conv.prompt_echo_off(c"Password:").unwrap().as_bytes(),
            b"secret"
        );
        assert!(conv.password.is_empty());
        assert!(conv.prompt_echo_off(c"OTP:").is_err());
        assert!(conv.radio_prompt(c"Continue?").is_err());
    }
}
