# idiolect-community

`idiolect-community` is the file-backed community control plane used by the
Idiolect CLI and Fieldwork. It treats a governed community change—not an
individual schema file—as the unit of work.

The crate provides:

- `idiolect.toml` workspace manifests;
- Panproto-backed consequence reports;
- configurable review and decision rules;
- three-state verification evidence;
- durable migration-run checkpoints;
- signed release bundles; and
- portable directory exports for migration between hosts or community forks.

See the Idiolect book's **Start a community workspace** path for the complete
command-line workflow.
