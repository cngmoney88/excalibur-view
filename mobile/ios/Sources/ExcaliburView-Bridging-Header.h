// What the Rust library (crates/mobile/src/ios.rs) offers the Swift around it.

#include <stddef.h>
#include <stdint.h>

/// Starts the program. Never returns: the thread is UIKit's from here on.
void excalibur_view_main(void);

/// The document picker's answer to request `asked`: the files, already copied
/// into the folder the program asked for. None when the picker was put away.
void exv_imported(uint64_t asked, const char *const *paths, size_t count);

/// Drawings opened with Excalibur View from another app, already copied into
/// the app's Documents.
void exv_opened(const char *const *paths, size_t count);
