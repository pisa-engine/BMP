mod common_index_format_v1;
pub use common_index_format_v1::{DocRecord, Header, Posting, PostingsList};

use std::fmt;

impl fmt::Display for Header {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(fmt, "----- CIFF HEADER -----")?;
        writeln!(fmt, "Version: {}", self.get_version())?;
        writeln!(fmt, "No. Postings Lists: {}", self.get_num_postings_lists())?;
        writeln!(
            fmt,
            "Total Postings Lists: {}",
            self.get_total_postings_lists()
        )?;
        writeln!(fmt, "No. Documents: {}", self.get_num_docs())?;
        writeln!(fmt, "Total Documents: {}", self.get_total_docs())?;
        writeln!(
            fmt,
            "Total Terms in Collection {}",
            self.get_total_terms_in_collection()
        )?;
        writeln!(
            fmt,
            "Average Document Length: {}",
            self.get_average_doclength()
        )?;
        writeln!(fmt, "Description: {}", self.get_description())?;
        write!(fmt, "-----------------------")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_display_header() {
        let header = Header {
            version: 1,
            num_postings_lists: 13,
            num_docs: 39,
            total_postings_lists: 399,
            total_docs: 200,
            total_terms_in_collection: 888,
            average_doclength: 12.7,
            description: "Test description".to_string(),
            ..Header::default()
        };
        let formatted = format!("{}", header);
        assert_eq!(
            formatted,
            r#"----- CIFF HEADER -----
Version: 1
No. Postings Lists: 13
Total Postings Lists: 399
No. Documents: 39
Total Documents: 200
Total Terms in Collection 888
Average Document Length: 12.7
Description: Test description
-----------------------"#
        );
    }
}
