## REMOVED Requirements

### Requirement: PROJECTS sidebar section

**Reason:** Project list and create / open / rename / delete moved to a File → Projects overlay so the sidebar stays TERMINALS and FILES only.

**Migration:** Use File → Projects… for the same actions. Engine Project commands are unchanged.

## ADDED Requirements

### Requirement: File Projects overlay

The UI SHALL provide a **File → Projects…** command that opens an overlay listing saved Projects. The user SHALL be able to open, create, rename, and delete Projects from that overlay. The sidebar SHALL NOT list Projects. When a Project is active, the sidebar SHALL show that Project’s name above the TERMINALS section. Selection chrome SHALL NOT use accent borders (background tint only if needed).

#### Scenario: Open from list

- **WHEN** the user activates a Project in the File → Projects list
- **THEN** the engine opens that Project
- **AND** TERMINALS reflects the restored Programs
- **AND** the overlay closes

#### Scenario: Create from overlay

- **WHEN** the user creates a Project from the overlay
- **THEN** the engine creates an empty Project and opens it
- **AND** the overlay list includes the new Project

#### Scenario: Active project name above TERMINALS

- **WHEN** a Project is active
- **THEN** the sidebar shows that Project’s name above the TERMINALS section
- **AND** the name is hidden when no Project is active
