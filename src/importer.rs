#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlaylistImportEditor {
    pub url: String,
    pub loading: bool,
}

impl PlaylistImportEditor {
    pub fn input(&mut self, value: char) {
        if !self.loading && !value.is_control() && self.url.chars().count() < 2_048 {
            self.url.push(value);
        }
    }

    pub fn backspace(&mut self) {
        if !self.loading {
            self.url.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_accepts_a_real_url_and_ignores_newlines() {
        let mut editor = PlaylistImportEditor::default();
        for value in "https://soundcloud.com/user/sets/mix\n".chars() {
            editor.input(value);
        }

        assert_eq!(editor.url, "https://soundcloud.com/user/sets/mix");
    }
}
