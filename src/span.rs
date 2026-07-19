//! ソース位置情報。

/// ソース中のバイト範囲 (半開区間)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Span { start, end }
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// 1つのソースファイル。行・桁 (1始まり) の計算を提供する。
pub struct SourceFile {
    pub name: String,
    pub src: String,
    /// 各行の開始バイトオフセット。
    line_starts: Vec<u32>,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, src: impl Into<String>) -> Self {
        let src = src.into();
        let mut line_starts = vec![0u32];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push((i + 1) as u32);
            }
        }
        SourceFile {
            name: name.into(),
            src,
            line_starts,
        }
    }

    /// バイトオフセットから (行, 桁) を返す。どちらも1始まり。桁は文字数で数える。
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let line_start = self.line_starts[line_idx] as usize;
        let offset = (offset as usize).min(self.src.len());
        let col = self.src[line_start..offset].chars().count() as u32 + 1;
        (line_idx as u32 + 1, col)
    }

    /// 指定行 (1始まり) のテキストを改行なしで返す。
    pub fn line_text(&self, line: u32) -> &str {
        let idx = (line - 1) as usize;
        let start = self.line_starts[idx] as usize;
        let end = self
            .line_starts
            .get(idx + 1)
            .map(|&o| o as usize)
            .unwrap_or(self.src.len());
        self.src[start..end].trim_end_matches(['\n', '\r'])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_basic() {
        let f = SourceFile::new("t.tcrf", "abc\ndef\n");
        assert_eq!(f.line_col(0), (1, 1));
        assert_eq!(f.line_col(2), (1, 3));
        assert_eq!(f.line_col(4), (2, 1));
        assert_eq!(f.line_col(6), (2, 3));
    }

    #[test]
    fn line_col_multibyte() {
        let f = SourceFile::new("t.tcrf", "あい\nx");
        // "あ" は3バイト。オフセット3は2文字目の先頭 → 桁2。
        assert_eq!(f.line_col(3), (1, 2));
        assert_eq!(f.line_col(7), (2, 1));
    }

    #[test]
    fn line_text_strips_newline() {
        let f = SourceFile::new("t.tcrf", "abc\r\ndef");
        assert_eq!(f.line_text(1), "abc");
        assert_eq!(f.line_text(2), "def");
    }
}
