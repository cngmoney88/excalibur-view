//! Something to try the app on.
//!
//! The first time the app runs it puts a drawing set and a steel model in a
//! Samples folder among the drawings: the fictional Northgate warehouse, the
//! same set the website's pictures are taken from, and its model. Somebody who
//! has just installed it can open a sheet, mark it up and measure off it
//! before they have found where their own drawings are. So can an app store's
//! reviewer.
//!
//! Once only. A marker file says it was done, so samples somebody deleted do
//! not come back.

const SET: (&str, &[u8]) = (
    "Northgate Warehouse - Structural Set.pdf",
    include_bytes!("../samples/Northgate Warehouse - Structural Set.pdf"),
);
const MODEL: (&str, &[u8]) = (
    "Demo Warehouse and Office.ifc",
    include_bytes!("../samples/Demo Warehouse and Office.ifc"),
);

pub fn put_in_place() {
    let drawings = hyperview::files::drawings_folder();
    let marker = drawings.join(".samples-given");
    if marker.exists() {
        return;
    }
    let folder = drawings.join("Samples");
    if let Err(e) = std::fs::create_dir_all(&folder) {
        log::warn!("no samples: {} could not be made: {e}", folder.display());
        return;
    }
    for (name, bytes) in [SET, MODEL] {
        let path = folder.join(name);
        if !path.exists() {
            if let Err(e) = std::fs::write(&path, bytes) {
                log::warn!("{} could not be written: {e}", path.display());
            }
        }
    }
    let _ = std::fs::write(&marker, b"The samples were put here once. Deleting them keeps them gone.\n");
}
