use anyhow::{Result, bail};

use crate::cli::NoteCommand;
use crate::graph::GraphFile;
use crate::schema::GraphSchema;
use crate::storage::GraphStore;

pub(crate) struct GraphNoteContext<'a> {
    pub(crate) path: &'a std::path::Path,
    pub(crate) graph_file: &'a mut GraphFile,
    pub(crate) store: &'a dyn GraphStore,
    pub(crate) _schema: Option<&'a GraphSchema>,
}

pub(crate) fn execute_note(command: NoteCommand, context: GraphNoteContext<'_>) -> Result<String> {
    match command {
        NoteCommand::Add(args) => {
            let note = crate::build_note(context.graph_file, args)?;
            context.graph_file.notes.push(note.clone());
            context.store.save_graph(context.path, context.graph_file)?;
            crate::append_event_snapshot(
                context.path,
                "note.add",
                Some(note.id.clone()),
                context.graph_file,
            )?;
            Ok(format!("+ note {}\n", note.id))
        }
        NoteCommand::List(args) => Ok(crate::render_note_list(context.graph_file, &args)),
        NoteCommand::Remove { id } => {
            let before = context.graph_file.notes.len();
            context.graph_file.notes.retain(|note| note.id != id);
            let removed = before.saturating_sub(context.graph_file.notes.len());
            if removed == 0 {
                bail!("note not found: {id}");
            }
            context.store.save_graph(context.path, context.graph_file)?;
            crate::append_event_snapshot(
                context.path,
                "note.remove",
                Some(id.clone()),
                context.graph_file,
            )?;
            Ok(format!("- note {id}\n"))
        }
    }
}
