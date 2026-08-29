## Purpose

Engine support for opening multi-gigabyte log files without holding every line offset in RAM, and for whole-file include/exclude/search via a match-offset index over the original file.

## Requirements

### Requirement: Sparse or on-disk line index

For file sessions, the engine SHALL NOT require an in-memory offset for every line of the entire file. Opening and scrolling a multi-gigabyte log SHALL use a sparse and/or on-disk line index so memory does not grow linearly with total line count at full density.

#### Scenario: Large file opens without full offset vector

- **WHEN** the user opens a multi-gigabyte log file
- **THEN** the session becomes usable for scrolling and viewing
- **AND** the engine does not allocate one in-RAM offset entry per line for the whole file

### Requirement: Match index for file filter tabs

When a filter Tab on a file session has include/exclude (and/or severity) rules that restrict visibility, the engine SHALL build a match index of byte offsets (or equivalent stable positions) for matching lines or Records in the **entire** source file. The Viewport for that Tab SHALL present a continuous scroll over matches by seeking and reading from the original file. The engine SHALL NOT materialize a full text copy of all matching lines for the whole file in v1.

#### Scenario: Sparse include over whole file

- **WHEN** the user adds an include rule on a filter Tab of a file session
- **AND** matching Records exist outside the previously loaded sliding window
- **THEN** those matches are discoverable by scrolling the filtered Tab
- **AND** match text is read from the original file on demand

#### Scenario: Empty match set

- **WHEN** a filter Tab's rules match no lines in the file
- **THEN** the Viewport shows an empty filtered view
- **AND** the session remains responsive

#### Scenario: Rule change rescans

- **WHEN** the user changes include/exclude or severity on a file filter Tab
- **THEN** the previous match scan is superseded
- **AND** a new match index is built for the updated rules

### Requirement: Search match index on files

Search on a file session SHALL be able to find matches across the whole file using a match-index (or equivalent whole-file) approach, not only the in-memory window.

#### Scenario: Search hits outside window

- **WHEN** the user searches on a file session for a string that occurs only far from the current window
- **THEN** the search can locate that occurrence
