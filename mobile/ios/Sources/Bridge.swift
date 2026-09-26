import UIKit
import UniformTypeIdentifiers
import Vision

// What the program asks of UIKit: the document picker, the share sheet,
// printing, opening a link, and reading words with Vision. Each is a C function
// the Rust library calls (crates/mobile/src/ios.rs declares them), and each
// does its work on the main thread, where UIKit wants it.

// ---- where things are ------------------------------------------------------------

/// The app's Documents folder: what the Files app shows as Excalibur View's,
/// and where the program keeps drawings.
let documents: URL = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]

/// The view controller everything is presented from.
func topController() -> UIViewController? {
    var windows = UIApplication.shared.connectedScenes
        .compactMap { $0 as? UIWindowScene }
        .flatMap { $0.windows }
    if windows.isEmpty {
        // An app that has not adopted scenes may have its window outside any.
        windows = UIApplication.shared.windows
    }
    let window = windows.first(where: { $0.isKeyWindow }) ?? windows.first
    var top = window?.rootViewController
    while let presented = top?.presentedViewController {
        top = presented
    }
    return top
}

/// On an iPad a popover has to point at something. With nothing in particular
/// to point at, it points at the middle of the screen.
func anchor(_ controller: UIViewController, over host: UIViewController) {
    guard let popover = controller.popoverPresentationController else { return }
    popover.sourceView = host.view
    popover.sourceRect = CGRect(x: host.view.bounds.midX, y: host.view.bounds.midY, width: 1, height: 1)
    popover.permittedArrowDirections = []
}

/// A name for a file that isn't taken in `folder`: the name itself when it is
/// free, and "name 2", "name 3" when not. The same rule the program uses.
func freeURL(in folder: URL, name: String) -> URL {
    let wanted = folder.appendingPathComponent(name)
    if !FileManager.default.fileExists(atPath: wanted.path) {
        return wanted
    }
    let stem = wanted.deletingPathExtension().lastPathComponent
    let ext = wanted.pathExtension
    var n = 2
    while true {
        let candidate = folder.appendingPathComponent(ext.isEmpty ? "\(stem) \(n)" : "\(stem) \(n).\(ext)")
        if !FileManager.default.fileExists(atPath: candidate.path) {
            return candidate
        }
        n += 1
    }
}

/// Hands a list of paths to a C function that takes `const char *const *`.
func withCStrings(_ strings: [String], _ body: (UnsafePointer<UnsafePointer<CChar>?>?, Int) -> Void) {
    if strings.isEmpty {
        body(nil, 0)
        return
    }
    let owned: [UnsafeMutablePointer<CChar>?] = strings.map { strdup($0) }
    defer { owned.forEach { free($0) } }
    let borrowed: [UnsafePointer<CChar>?] = owned.map { $0.map { UnsafePointer($0) } }
    borrowed.withUnsafeBufferPointer { buffer in
        body(buffer.baseAddress, buffer.count)
    }
}

// ---- drawings opened with the app ---------------------------------------------------

/// Takes drawings opened with Excalibur View from another app. A document
/// lent to the app is copied into Documents/Inbox, among the drawings, and the
/// copy handed to the program, which opens it as a tab.
final class Opener: NSObject, UIApplicationDelegate {
    func application(_ app: UIApplication, open url: URL,
                     options: [UIApplication.OpenURLOptionsKey: Any] = [:]) -> Bool {
        take([url])
        return true
    }

    func take(_ urls: [URL]) {
        let inbox = documents.appendingPathComponent("Inbox")
        try? FileManager.default.createDirectory(at: inbox, withIntermediateDirectories: true)
        var taken: [String] = []
        for url in urls where url.isFileURL {
            // Already among the drawings (the system copied it there itself):
            // opened where it is.
            if url.standardizedFileURL.path.hasPrefix(documents.standardizedFileURL.path) {
                taken.append(url.path)
                continue
            }
            let lent = url.startAccessingSecurityScopedResource()
            defer { if lent { url.stopAccessingSecurityScopedResource() } }
            let copy = freeURL(in: inbox, name: url.lastPathComponent)
            do {
                try FileManager.default.copyItem(at: url, to: copy)
                taken.append(copy.path)
            } catch {
                NSLog("Excalibur View could not copy \(url): \(error)")
            }
        }
        if !taken.isEmpty {
            withCStrings(taken) { exv_opened($0, $1) }
        }
    }
}

// ---- the document picker ------------------------------------------------------------

/// One picker, and the request it answers.
final class Picking: NSObject, UIDocumentPickerDelegate {
    let asked: UInt64
    let into: URL
    /// Kept alive while the picker is open: the picker holds its delegate weakly.
    static var open: [Picking] = []

    init(asked: UInt64, into: URL) {
        self.asked = asked
        self.into = into
    }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        try? FileManager.default.createDirectory(at: into, withIntermediateDirectories: true)
        var brought: [String] = []
        for url in urls {
            // Picked "as copy": these are the app's own temporary copies,
            // moved rather than copied again.
            let lent = url.startAccessingSecurityScopedResource()
            defer { if lent { url.stopAccessingSecurityScopedResource() } }
            let destination = freeURL(in: into, name: url.lastPathComponent)
            do {
                try FileManager.default.moveItem(at: url, to: destination)
                brought.append(destination.path)
            } catch {
                do {
                    try FileManager.default.copyItem(at: url, to: destination)
                    brought.append(destination.path)
                } catch {
                    NSLog("Excalibur View could not bring in \(url): \(error)")
                }
            }
        }
        finish(brought)
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        finish([])
    }

    private func finish(_ paths: [String]) {
        withCStrings(paths) { exv_imported(asked, $0, $1) }
        Picking.open.removeAll { $0 === self }
    }
}

@_cdecl("exv_ios_import")
public func exvImport(_ asked: UInt64, _ types: UnsafePointer<CChar>?, _ many: Bool, _ into: UnsafePointer<CChar>) {
    let mimes = types.map { String(cString: $0) }?
        .split(separator: ",")
        .map(String.init) ?? []
    let folder = URL(fileURLWithPath: String(cString: into), isDirectory: true)
    DispatchQueue.main.async {
        var kinds = mimes.compactMap { UTType(mimeType: $0) }
        if kinds.isEmpty {
            kinds = [.item]
        }
        guard let host = topController() else {
            withCStrings([]) { exv_imported(asked, $0, $1) }
            return
        }
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: kinds, asCopy: true)
        picker.allowsMultipleSelection = many
        let picking = Picking(asked: asked, into: folder)
        Picking.open.append(picking)
        picker.delegate = picking
        host.present(picker, animated: true)
    }
}

// ---- sending a copy, printing, links ----------------------------------------------

@_cdecl("exv_ios_share")
public func exvShare(_ path: UnsafePointer<CChar>) {
    let url = URL(fileURLWithPath: String(cString: path))
    DispatchQueue.main.async {
        guard let host = topController() else { return }
        let sheet = UIActivityViewController(activityItems: [url], applicationActivities: nil)
        anchor(sheet, over: host)
        host.present(sheet, animated: true)
    }
}

@_cdecl("exv_ios_print")
public func exvPrint(_ path: UnsafePointer<CChar>, _ name: UnsafePointer<CChar>) {
    let url = URL(fileURLWithPath: String(cString: path))
    let job = String(cString: name)
    DispatchQueue.main.async {
        let printing = UIPrintInteractionController.shared
        let info = UIPrintInfo(dictionary: nil)
        info.outputType = .general
        info.jobName = job
        printing.printInfo = info
        printing.printingItem = url
        if let host = topController(), let view = host.view {
            let middle = CGRect(x: view.bounds.midX, y: view.bounds.midY, width: 1, height: 1)
            printing.present(from: middle, in: view, animated: true, completionHandler: nil)
        } else {
            printing.present(animated: true, completionHandler: nil)
        }
    }
}

@_cdecl("exv_ios_open_url")
public func exvOpenURL(_ address: UnsafePointer<CChar>) {
    guard let url = URL(string: String(cString: address)) else { return }
    DispatchQueue.main.async {
        UIApplication.shared.open(url)
    }
}

// ---- reading words with Vision ------------------------------------------------------

/// Reads the words off a greyscale picture, one byte a pixel, with Vision's
/// text recogniser, which runs on the iPad and sends nothing anywhere. Called
/// from one of the program's own threads, which may wait.
///
/// One line a word: left, top, right, bottom in pixels from the top left,
/// confidence, and the word, separated by tabs. Memory from strdup, which the
/// caller frees. Nil when the picture could not be read.
@_cdecl("exv_ios_read_words")
public func exvReadWords(_ grey: UnsafePointer<UInt8>, _ width: Int32, _ height: Int32) -> UnsafeMutablePointer<CChar>? {
    let w = Int(width)
    let h = Int(height)
    guard w > 0, h > 0,
          let provider = CGDataProvider(data: Data(bytes: grey, count: w * h) as CFData),
          let picture = CGImage(width: w, height: h, bitsPerComponent: 8, bitsPerPixel: 8, bytesPerRow: w,
                                space: CGColorSpaceCreateDeviceGray(),
                                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.none.rawValue),
                                provider: provider, decode: nil, shouldInterpolate: false,
                                intent: .defaultIntent)
    else {
        return nil
    }
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    // Drawings are part numbers, grid lines and abbreviations. Corrected to
    // dictionary words they would be wrong.
    request.usesLanguageCorrection = false
    do {
        try VNImageRequestHandler(cgImage: picture, options: [:]).perform([request])
    } catch {
        NSLog("Excalibur View could not read the words: \(error)")
        return nil
    }
    var lines = ""
    for observation in request.results ?? [] {
        guard let candidate = observation.topCandidates(1).first else { continue }
        let text = candidate.string
        // Word by word, split where the spaces are: "HSS4X4X1/4" is one word
        // on a drawing, whatever a dictionary thinks.
        var start = text.startIndex
        while start < text.endIndex {
            while start < text.endIndex && text[start].isWhitespace {
                start = text.index(after: start)
            }
            if start >= text.endIndex { break }
            var end = start
            while end < text.endIndex && !text[end].isWhitespace {
                end = text.index(after: end)
            }
            let range = start..<end
            if let box = try? candidate.boundingBox(for: range)?.boundingBox {
                let left = Int(box.minX * CGFloat(w))
                let right = Int(box.maxX * CGFloat(w))
                let top = Int((1 - box.maxY) * CGFloat(h))
                let bottom = Int((1 - box.minY) * CGFloat(h))
                let word = text[range].replacingOccurrences(of: "\t", with: " ")
                lines += "\(left)\t\(top)\t\(right)\t\(bottom)\t\(candidate.confidence)\t\(word)\n"
            }
            start = end
        }
    }
    return strdup(lines)
}
