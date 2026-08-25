use crate::core::types::{FilterRule, FilterType, LogRecord};

pub struct FilterEngine {
    filters: Vec<FilterRule>,
}

impl Default for FilterEngine {
    fn default() -> Self {
        Self {
            filters: Vec::new(),
        }
    }
}

impl FilterEngine {
    pub fn new(filters: Vec<FilterRule>) -> Self {
        Self { filters }
    }

    pub fn set_filters(&mut self, filters: Vec<FilterRule>) {
        self.filters = filters;
    }

    pub fn filters(&self) -> &[FilterRule] {
        &self.filters
    }

    pub fn filters_mut(&mut self) -> &mut Vec<FilterRule> {
        &mut self.filters
    }

    pub fn is_visible(&self, record: &LogRecord) -> bool {
        let active: Vec<_> = self.filters.iter().filter(|f| f.enabled).collect();
        if active.is_empty() {
            return true;
        }

        let includes: Vec<_> = active
            .iter()
            .filter(|f| f.filter_type == FilterType::Include)
            .collect();
        let excludes: Vec<_> = active
            .iter()
            .filter(|f| f.filter_type == FilterType::Exclude)
            .collect();

        for filter in excludes {
            if filter
                .regex
                .as_ref()
                .is_some_and(|re| re.is_match(&record.text))
            {
                return false;
            }
        }

        if includes.is_empty() {
            return true;
        }

        includes.iter().any(|f| {
            f.regex
                .as_ref()
                .is_some_and(|re| re.is_match(&record.text))
        })
    }

    pub fn filter_records<'a>(&self, records: &'a [LogRecord]) -> Vec<&'a LogRecord> {
        records
            .iter()
            .filter(|r| self.is_visible(r))
            .collect()
    }
}
