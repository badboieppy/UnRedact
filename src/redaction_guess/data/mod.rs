pub mod dictionary_data;
pub mod font_run_data;
pub mod report_data;

pub use dictionary_data::{DictionaryData, DictionaryDataSource, DictionaryInputs};
pub use font_run_data::{FontRunData, FontRunDataSource, FontRunInputs};
pub use report_data::{GuessReportInputs, ReportData, ReportDataSource};
