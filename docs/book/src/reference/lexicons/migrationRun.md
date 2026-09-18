# `dev.idiolect.migrationRun`

`migrationRun` publishes durable operational state for applying one governed change.

Required fields are `community`, `change`, `status`, `processed`, `failed`, `createdAt`, and `updatedAt`. Status is an open enum with known values `planned`, `running`, `paused`, `completed`, `failed`, and `rolled-back`.

`total` may be absent when the corpus size is not known. `lens` may cite the published translation used by the run. Checkpoints contain a cursor, cumulative processed count, and recording time. Failure entries contain a record identifier and reason.

The `failures` array is a bounded diagnostic sample. It should not be interpreted as the complete failed corpus unless a community convention explicitly says so.
