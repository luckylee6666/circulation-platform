use std::io::Cursor;

use calamine::{Data, Reader, open_workbook_auto_from_rs};

use crate::error::{AppError, AppResult};

/// 单次导入的行数上限，避免有人误传一个几十万行的表把内存吃满。
pub const MAX_ROWS: usize = 100_000;
pub const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug)]
pub struct ParsedSheet {
    pub source_name: String,
    pub rows: Vec<Vec<String>>,
}

/// 解析上传的表格文件（Excel / CSV）。
pub fn from_upload(file_name: &str, bytes: &[u8]) -> AppResult<ParsedSheet> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AppError::bad_request(format!(
            "文件超过 {} MB，请拆分后再导入",
            MAX_FILE_BYTES / 1024 / 1024
        )));
    }

    let extension = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();

    let rows = match extension.as_str() {
        "xlsx" | "xlsm" | "xlsb" | "xls" | "ods" => parse_workbook(bytes)?,
        "csv" | "txt" => parse_delimited(&decode_text(bytes), b',')?,
        "" => return Err(AppError::bad_request("文件没有扩展名，无法判断格式")),
        other => {
            return Err(AppError::bad_request(format!(
                "不支持 .{other} 格式，请另存为 Excel 或 CSV 后再导入"
            )));
        }
    };

    finalize(file_name, rows)
}

/// 解析从 Excel 直接复制粘贴的文本（制表符分隔）。
pub fn from_paste(text: &str) -> AppResult<ParsedSheet> {
    if text.trim().is_empty() {
        return Err(AppError::bad_request("粘贴的内容是空的"));
    }
    let rows = parse_delimited(text, b'\t')?;
    finalize("粘贴的数据", rows)
}

fn finalize(source_name: &str, mut rows: Vec<Vec<String>>) -> AppResult<ParsedSheet> {
    // 去掉整行为空的行（表格末尾经常拖一堆空行）
    rows.retain(|row| row.iter().any(|cell| !cell.trim().is_empty()));

    if rows.is_empty() {
        return Err(AppError::bad_request("没有读到任何数据"));
    }
    if rows.len() > MAX_ROWS {
        return Err(AppError::bad_request(format!(
            "数据有 {} 行，超过单次导入上限 {MAX_ROWS} 行，请分批导入",
            rows.len()
        )));
    }

    // 各行列数对齐，后面按列号取值时才不会越界
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut rows {
        row.resize(width, String::new());
    }

    Ok(ParsedSheet {
        source_name: source_name.to_string(),
        rows,
    })
}

fn parse_workbook(bytes: &[u8]) -> AppResult<Vec<Vec<String>>> {
    let mut workbook = open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|err| AppError::bad_request(format!("无法打开表格文件：{err}")))?;

    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| AppError::bad_request("表格里没有工作表"))?
        .map_err(|err| AppError::bad_request(format!("读取工作表失败：{err}")))?;

    Ok(range
        .rows()
        .map(|row| row.iter().map(cell_to_string).collect())
        .collect())
}

fn parse_delimited(text: &str, delimiter: u8) -> AppResult<Vec<Vec<String>>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(text.as_bytes());

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|err| AppError::bad_request(format!("解析文本失败：{err}")))?;
        rows.push(record.iter().map(|cell| cell.trim().to_string()).collect());
    }
    Ok(rows)
}

/// 中文 Windows 导出的 CSV 多数是 GBK，UTF-8 解不开时就按 GBK 再试一次。
fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode(rest).0.into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode(rest).0.into_owned();
    }

    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => encoding_rs::GBK.decode(bytes).0.into_owned(),
    }
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(value) => value.trim().to_string(),
        Data::Int(value) => value.to_string(),
        // Excel 内部一律是浮点，整数不要显示成 1234.0
        Data::Float(value) => {
            if value.fract() == 0.0 && value.abs() < 1e15 {
                format!("{}", *value as i64)
            } else {
                value.to_string()
            }
        }
        Data::Bool(value) => if *value { "是" } else { "否" }.to_string(),
        Data::DateTime(datetime) => format_datetime(datetime),
        Data::DateTimeIso(value) => value.clone(),
        Data::DurationIso(value) => value.clone(),
        Data::Error(err) => format!("#错误({err:?})"),
    }
}

fn format_datetime(datetime: &calamine::ExcelDateTime) -> String {
    match datetime.as_datetime() {
        Some(value) => {
            // 纯日期不要拖一个 00:00:00 的尾巴
            let midnight = value.time() == chrono::NaiveTime::MIN;
            if midnight {
                value.format("%Y-%m-%d").to_string()
            } else {
                value.format("%Y-%m-%d %H:%M:%S").to_string()
            }
        }
        None => datetime.to_string(),
    }
}

/// 猜表头所在行：第一个「有两个以上非空单元格」的行。
/// 很多系统导出的表格前面会有标题行，所以不能想当然认为第一行就是表头。
pub fn suggest_header_row(rows: &[Vec<String>]) -> usize {
    rows.iter()
        .position(|row| row.iter().filter(|cell| !cell.trim().is_empty()).count() >= 2)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_tsv_is_parsed() {
        let sheet = from_paste("编号\t名称\t数量\nA-1\t螺丝\t10\nA-2\t螺母\t20").unwrap();
        assert_eq!(sheet.rows.len(), 3);
        assert_eq!(sheet.rows[0], vec!["编号", "名称", "数量"]);
        assert_eq!(sheet.rows[2][1], "螺母");
    }

    #[test]
    fn trailing_blank_rows_are_dropped() {
        let sheet = from_paste("编号\t名称\nA-1\t螺丝\n\t\n").unwrap();
        assert_eq!(sheet.rows.len(), 2);
    }

    #[test]
    fn ragged_rows_are_padded() {
        let sheet = from_paste("a\tb\tc\n1\t2").unwrap();
        assert_eq!(sheet.rows[1], vec!["1", "2", ""]);
    }

    #[test]
    fn csv_is_decoded_as_gbk_when_not_utf8() {
        // “编号,名称\nA-1,螺丝” 的 GBK 字节
        let gbk: Vec<u8> = vec![
            0xB1, 0xE0, 0xBA, 0xC5, b',', 0xC3, 0xFB, 0xB3, 0xC6, b'\n', b'A', b'-', b'1', b',',
            0xC2, 0xDD, 0xCB, 0xBF,
        ];
        let sheet = from_upload("x.csv", &gbk).unwrap();
        assert_eq!(sheet.rows[0], vec!["编号", "名称"]);
        assert_eq!(sheet.rows[1][1], "螺丝");
    }

    #[test]
    fn utf8_bom_is_stripped() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("编号,名称\nA-1,螺丝".as_bytes());
        let sheet = from_upload("x.csv", &bytes).unwrap();
        assert_eq!(sheet.rows[0][0], "编号");
    }

    #[test]
    fn unsupported_extension_is_rejected() {
        let error = from_upload("notes.pdf", b"whatever").unwrap_err();
        assert!(error.message().contains("pdf"));
    }

    #[test]
    fn empty_paste_is_rejected() {
        assert!(from_paste("   \n  ").is_err());
    }

    #[test]
    fn header_row_skips_title_rows() {
        let rows = vec![
            vec!["2026 年第一季度数据统计表".to_string(), String::new(), String::new()],
            vec!["编号".into(), "名称".into(), "数量".into()],
            vec!["A-1".into(), "螺丝".into(), "10".into()],
        ];
        assert_eq!(suggest_header_row(&rows), 1);
    }

    #[test]
    fn header_row_falls_back_to_first_row() {
        let rows = vec![vec!["只有一个格子".to_string()], vec!["另一个".to_string()]];
        assert_eq!(suggest_header_row(&rows), 0);
    }
}
