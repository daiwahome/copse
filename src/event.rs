use crossterm::event::KeyEvent;
use uuid::Uuid;

pub type TaskId = Uuid;

pub enum AppEvent {
    Key(KeyEvent),
    /// PTY produced output (parser already updated) — triggers a redraw
    TaskOutput(TaskId),
    /// Task process has exited
    TaskExited(TaskId),
    /// Terminal was resized
    Resize { cols: u16, rows: u16 },
    /// A new task entry was created (Stopped, no worktree yet)
    TaskCreated(crate::task::Task),
    /// A stopped task was resumed; replace the matching task by ID
    TaskResumed {
        id: TaskId,
        result: Result<crate::task::Task, String>,
    },
    /// Task was deleted (worktree + branch removed); remove from list by ID
    TaskDeleted(TaskId),
    /// Result of an async git operation: Ok(()) to refresh, Err(msg) for error
    GitOpResult(Result<(), String>),
    /// Squash merge requested — needs alternate screen exit for $EDITOR
    SquashMerge { name: String, upstream: String },
    /// Unrecoverable error in a background task; triggers shutdown
    FatalError(String),
}
