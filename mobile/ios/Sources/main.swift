import UIKit

// Excalibur View on the iPad.
//
// The program is the Rust library linked into this app (crates/mobile). This
// file only starts it, after making sure a drawing opened with Excalibur View
// from another app - Mail, Files, a browser's download - finds its way in.
//
// winit, which the program draws its window with, deliberately leaves the
// application delegate unset for the app to fill in. The Opener below takes
// that place as soon as the app has launched; the only thing it does is take
// drawings in. Everything else UIKit tells an app, winit hears through
// notifications, so nothing is taken away from it.

let opener = Opener()

NotificationCenter.default.addObserver(
    forName: UIApplication.didFinishLaunchingNotification,
    object: nil,
    queue: .main
) { note in
    UIApplication.shared.delegate = opener
    // A drawing that started the app arrives with the launch, not afterwards.
    if let url = note.userInfo?[UIApplication.LaunchOptionsKey.url] as? URL {
        opener.take([url])
    }
}

excalibur_view_main()
