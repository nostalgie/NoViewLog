## MODIFIED Requirements

### Requirement: Persist Projects store

Projects and Programs SHALL persist in the user projects store (`projects.yaml`). Creating a Project SHALL start with zero Programs (it SHALL NOT snapshot live TERMINALS). After create, the engine SHALL open that Project. While a Project is active, changes to program order, titles, launch, and tabs SHALL be saved back to that Project.

#### Scenario: Create starts empty

- **WHEN** the user creates a Project while live TERMINALS have launch commands and extra tabs
- **THEN** the new Project has zero Programs
- **AND** TERMINALS is replaced with one stopped Terminal that does not copy those launches or tabs
