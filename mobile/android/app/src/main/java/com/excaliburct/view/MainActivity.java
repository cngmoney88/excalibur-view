package com.excaliburct.view;

import android.content.ActivityNotFoundException;
import android.content.ClipData;
import android.content.Intent;
import android.database.Cursor;
import android.graphics.Color;
import android.net.Uri;
import android.os.Bundle;
import android.os.CancellationSignal;
import android.os.ParcelFileDescriptor;
import android.print.PageRange;
import android.print.PrintAttributes;
import android.print.PrintDocumentAdapter;
import android.print.PrintDocumentInfo;
import android.print.PrintManager;
import android.provider.OpenableColumns;
import android.system.ErrnoException;
import android.system.Os;
import android.util.Log;
import android.view.View;
import android.webkit.MimeTypeMap;

import androidx.activity.EdgeToEdge;
import androidx.activity.SystemBarStyle;
import androidx.core.content.FileProvider;
import androidx.core.graphics.Insets;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowInsetsCompat;

import com.google.androidgamesdk.GameActivity;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * The one window. GameActivity loads the Rust library named in the manifest
 * and runs the program on a thread of its own; this class prepares the ground
 * before that, keeps the drawing clear of the system bars, and does for the
 * program what only an activity can: the system's file picker, the share
 * sheet, printing, and drawings opened with the app from elsewhere. See
 * {@link Bridge}.
 */
public class MainActivity extends GameActivity {
    private static final String TAG = "ExcaliburView";

    /** The activity the program is running in, for {@link Bridge}. */
    static volatile MainActivity current;

    /** Copying files in and out, off the activity's thread. */
    private final ExecutorService copying = Executors.newSingleThreadExecutor();

    /** Pickers open: the request code, and what the program asked for. */
    private final Map<Integer, Picking> picking = new HashMap<>();
    private int nextRequest = 7000;

    private static final class Picking {
        final long asked;
        final String into;

        Picking(long asked, String into) {
            this.asked = asked;
            this.into = into;
        }
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        // Before super.onCreate, which is where the library is loaded and the
        // program started, and before a drawing opened with the app is handed
        // over, which the program's folders have to be known for.
        //
        // TMPDIR: Rust's temporary folder on Android is /data/local/tmp, which
        // an app cannot write to. The cache folder is the app's own.
        //
        // PDFIUM_DIR: the folder the native libraries were installed into. The
        // PDF engine is loaded by name from there; the tile helpers each copy
        // it from there so they can draw at the same time (render.rs).
        //
        // HOME and the XDG folders: an app has no home folder, so the program's
        // settings, profiles and inbox go in its private one.
        //
        // EXV_DRAWINGS: where the program keeps drawings. The app-specific
        // folder on shared storage when there is one, so a computer on a USB
        // cable can reach them; the private one otherwise.
        File files = getFilesDir();
        setEnvironment("TMPDIR", getCacheDir().getAbsolutePath());
        setEnvironment("PDFIUM_DIR", getApplicationInfo().nativeLibraryDir);
        setEnvironment("HOME", files.getAbsolutePath());
        setEnvironment("XDG_DATA_HOME", new File(files, ".local/share").getAbsolutePath());
        setEnvironment("XDG_CONFIG_HOME", new File(files, ".config").getAbsolutePath());
        setEnvironment("XDG_CACHE_HOME", getCacheDir().getAbsolutePath());
        File shared = getExternalFilesDir(null);
        File drawings = new File(shared != null ? shared : files, "Drawings");
        //noinspection ResultOfMethodCallIgnored
        drawings.mkdirs();
        setEnvironment("EXV_DRAWINGS", drawings.getAbsolutePath());

        // Android 15 and later draw every app edge to edge whatever it asks
        // for. Asking for it on every version keeps them all alike, and light
        // bar icons suit the dark chrome.
        EdgeToEdge.enable(this,
                SystemBarStyle.dark(Color.TRANSPARENT),
                SystemBarStyle.dark(Color.TRANSPARENT));

        super.onCreate(savedInstanceState);
        current = this;

        keepClearOfSystemBars();
        takeDrawing(getIntent());
    }

    @Override
    protected void onDestroy() {
        if (current == this) {
            current = null;
        }
        super.onDestroy();
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        takeDrawing(intent);
    }

    /**
     * Pads the view holding the drawing surface by the status bar, the
     * navigation bar, any camera cutout and, while it is up, the keyboard.
     * The surface shrinks to fit, so the program sees a smaller window rather
     * than having its menus and its text boxes drawn underneath them. The
     * strips left over show the window background, the program's own chrome.
     *
     * The insets are passed on unconsumed: GameActivity reads the keyboard's
     * from the same dispatch, on the surface view inside this one.
     */
    private void keepClearOfSystemBars() {
        View content = findViewById(contentViewId);
        if (content == null) {
            Log.w(TAG, "no content view to keep clear of the system bars");
            return;
        }
        ViewCompat.setOnApplyWindowInsetsListener(content, (view, insets) -> {
            Insets bars = insets.getInsets(WindowInsetsCompat.Type.systemBars()
                    | WindowInsetsCompat.Type.displayCutout());
            Insets keyboard = insets.getInsets(WindowInsetsCompat.Type.ime());
            view.setPadding(bars.left, bars.top, bars.right,
                    Math.max(bars.bottom, keyboard.bottom));
            return insets;
        });
        ViewCompat.requestApplyInsets(content);
    }

    // ---- a drawing opened with the app --------------------------------------

    /**
     * A PDF sent here with "open with" or "share". A content:// address is not
     * a file, so it is copied into the Inbox folder among the drawings and the
     * copy handed to the program, which opens it as a tab.
     */
    private void takeDrawing(Intent intent) {
        if (intent == null) {
            return;
        }
        List<Uri> sent = new ArrayList<>();
        String action = intent.getAction();
        if (Intent.ACTION_VIEW.equals(action) && intent.getData() != null) {
            sent.add(intent.getData());
        } else if (Intent.ACTION_SEND.equals(action)) {
            Uri one = intent.getParcelableExtra(Intent.EXTRA_STREAM);
            if (one != null) {
                sent.add(one);
            }
        } else if (Intent.ACTION_SEND_MULTIPLE.equals(action)) {
            ArrayList<Uri> many = intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM);
            if (many != null) {
                sent.addAll(many);
            }
        }
        if (sent.isEmpty()) {
            return;
        }
        // Handled once: a rotation or a return to the app must not open it again.
        intent.setAction(Intent.ACTION_MAIN);
        File inbox = new File(System.getenv("EXV_DRAWINGS"), "Inbox");
        copying.execute(() -> {
            List<String> copied = copyIn(sent, inbox);
            if (!copied.isEmpty()) {
                Bridge.nativeOpened(copied.toArray(new String[0]));
            }
        });
    }

    // ---- the system's picker ------------------------------------------------

    void pickFiles(long asked, String[] mimeTypes, boolean many, String into) {
        Intent pick = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        pick.addCategory(Intent.CATEGORY_OPENABLE);
        pick.setType("*/*");
        if (mimeTypes != null && mimeTypes.length > 0) {
            pick.putExtra(Intent.EXTRA_MIME_TYPES, mimeTypes);
        }
        pick.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, many);
        int request = nextRequest++;
        picking.put(request, new Picking(asked, into));
        try {
            startActivityForResult(pick, request);
        } catch (ActivityNotFoundException e) {
            picking.remove(request);
            Log.w(TAG, "no file picker on this device", e);
            Bridge.nativeImported(asked, new String[0]);
        }
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        Picking asked = picking.remove(request);
        if (asked == null) {
            return;
        }
        List<Uri> chosen = new ArrayList<>();
        if (result == RESULT_OK && data != null) {
            ClipData clip = data.getClipData();
            if (clip != null) {
                for (int i = 0; i < clip.getItemCount(); i++) {
                    chosen.add(clip.getItemAt(i).getUri());
                }
            } else if (data.getData() != null) {
                chosen.add(data.getData());
            }
        }
        File into = new File(asked.into);
        copying.execute(() -> {
            List<String> copied = copyIn(chosen, into);
            Bridge.nativeImported(asked.asked, copied.toArray(new String[0]));
        });
    }

    /** Copies documents into a folder, under names not already taken there. */
    private List<String> copyIn(List<Uri> documents, File folder) {
        List<String> copied = new ArrayList<>();
        //noinspection ResultOfMethodCallIgnored
        folder.mkdirs();
        for (Uri document : documents) {
            File to = new File(folder, freeName(folder, displayName(document)));
            try (InputStream in = getContentResolver().openInputStream(document);
                 OutputStream out = new FileOutputStream(to)) {
                if (in == null) {
                    continue;
                }
                copy(in, out);
                copied.add(to.getAbsolutePath());
            } catch (IOException | SecurityException e) {
                Log.w(TAG, "could not copy " + document, e);
                //noinspection ResultOfMethodCallIgnored
                to.delete();
            }
        }
        return copied;
    }

    private String displayName(Uri document) {
        String name = null;
        if ("content".equals(document.getScheme())) {
            try (Cursor row = getContentResolver().query(document,
                    new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
                if (row != null && row.moveToFirst()) {
                    name = row.getString(0);
                }
            } catch (RuntimeException e) {
                Log.w(TAG, "no name for " + document, e);
            }
        }
        if (name == null) {
            name = document.getLastPathSegment();
        }
        if (name == null || name.isEmpty()) {
            name = "Drawing.pdf";
        }
        return name.replace('/', '-').replace('\\', '-');
    }

    /** The name itself when it is free, and "name 2", "name 3" when not. */
    private static String freeName(File folder, String wanted) {
        if (!new File(folder, wanted).exists()) {
            return wanted;
        }
        int dot = wanted.lastIndexOf('.');
        String stem = dot > 0 ? wanted.substring(0, dot) : wanted;
        String ext = dot > 0 ? wanted.substring(dot) : "";
        for (int n = 2; ; n++) {
            String name = stem + " " + n + ext;
            if (!new File(folder, name).exists()) {
                return name;
            }
        }
    }

    private static void copy(InputStream in, OutputStream out) throws IOException {
        byte[] buffer = new byte[1 << 16];
        int read;
        while ((read = in.read(buffer)) > 0) {
            out.write(buffer, 0, read);
        }
    }

    // ---- sending a copy -------------------------------------------------------

    void share(String path) {
        File file = new File(path);
        Uri uri;
        try {
            uri = FileProvider.getUriForFile(this, getPackageName() + ".files", file);
        } catch (IllegalArgumentException e) {
            Log.w(TAG, "cannot share " + path, e);
            return;
        }
        String ext = MimeTypeMap.getFileExtensionFromUrl(file.getName());
        String type = ext == null ? null : MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext.toLowerCase());
        Intent send = new Intent(Intent.ACTION_SEND);
        send.setType(type != null ? type : "application/octet-stream");
        send.putExtra(Intent.EXTRA_STREAM, uri);
        send.putExtra(Intent.EXTRA_SUBJECT, file.getName());
        send.setClipData(ClipData.newRawUri(file.getName(), uri));
        send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        try {
            startActivity(Intent.createChooser(send, file.getName()));
        } catch (ActivityNotFoundException e) {
            Log.w(TAG, "nothing to share " + path + " with", e);
        }
    }

    void openUrl(String url) {
        try {
            startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
        } catch (ActivityNotFoundException e) {
            Log.w(TAG, "nothing opens " + url, e);
        }
    }

    // ---- printing -------------------------------------------------------------

    void print(String path, String name) {
        PrintManager printing = (PrintManager) getSystemService(PRINT_SERVICE);
        if (printing == null) {
            Log.w(TAG, "no printing on this device");
            return;
        }
        printing.print(name, new PdfFile(new File(path), name), null);
    }

    /** Hands an existing PDF to the print system as it is. */
    private static final class PdfFile extends PrintDocumentAdapter {
        private final File file;
        private final String name;

        PdfFile(File file, String name) {
            this.file = file;
            this.name = name;
        }

        @Override
        public void onLayout(PrintAttributes old, PrintAttributes wanted, CancellationSignal cancel,
                             LayoutResultCallback done, Bundle extras) {
            if (cancel.isCanceled()) {
                done.onLayoutCancelled();
                return;
            }
            done.onLayoutFinished(new PrintDocumentInfo.Builder(name + ".pdf")
                    .setContentType(PrintDocumentInfo.CONTENT_TYPE_DOCUMENT)
                    .setPageCount(PrintDocumentInfo.PAGE_COUNT_UNKNOWN)
                    .build(), true);
        }

        @Override
        public void onWrite(PageRange[] pages, ParcelFileDescriptor destination, CancellationSignal cancel,
                            WriteResultCallback done) {
            try (InputStream in = new FileInputStream(file);
                 OutputStream out = new FileOutputStream(destination.getFileDescriptor())) {
                copy(in, out);
                done.onWriteFinished(new PageRange[]{PageRange.ALL_PAGES});
            } catch (IOException e) {
                done.onWriteFailed(e.getMessage());
            }
        }
    }

    private static void setEnvironment(String name, String value) {
        if (value == null) {
            return;
        }
        try {
            Os.setenv(name, value, true);
        } catch (ErrnoException e) {
            Log.w(TAG, "could not set " + name, e);
        }
    }
}
