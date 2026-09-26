package com.excaliburct.view;

import android.content.Intent;
import android.graphics.Color;
import android.net.Uri;
import android.os.Bundle;
import android.system.ErrnoException;
import android.system.Os;
import android.util.Log;
import android.view.View;

import androidx.activity.EdgeToEdge;
import androidx.activity.SystemBarStyle;
import androidx.core.graphics.Insets;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowInsetsCompat;

import com.google.androidgamesdk.GameActivity;

/**
 * The one window. GameActivity loads the Rust library named in the manifest
 * and runs the program on a thread of its own; this class only prepares the
 * ground before that and keeps the drawing clear of the system bars after.
 */
public class MainActivity extends GameActivity {
    private static final String TAG = "ExcaliburView";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        // Before super.onCreate, which is where the library is loaded and the
        // program started. These are the two places only this side knows.
        //
        // TMPDIR: Rust's temporary folder on Android is /data/local/tmp, which
        // an app cannot write to. The cache folder is the app's own, and the
        // system empties it when space runs short, which suits temporary files.
        //
        // PDFIUM_DIR: the folder the native libraries were installed into. The
        // PDF engine is loaded by name from there; the tile helpers each copy
        // it from there so they can draw at the same time (render.rs).
        setEnvironment("TMPDIR", getCacheDir().getAbsolutePath());
        setEnvironment("PDFIUM_DIR", getApplicationInfo().nativeLibraryDir);

        // Android 15 and later draw every app edge to edge whatever it asks
        // for. Asking for it on every version keeps them all alike, and light
        // bar icons suit the dark chrome.
        EdgeToEdge.enable(this,
                SystemBarStyle.dark(Color.TRANSPARENT),
                SystemBarStyle.dark(Color.TRANSPARENT));

        super.onCreate(savedInstanceState);

        keepClearOfSystemBars();
        noteDrawing(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        noteDrawing(intent);
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

    /**
     * A PDF sent here with "open with". Not handed to the program yet: a
     * content:// address is not a file path, and has to be read through the
     * content resolver and copied somewhere the program can open. Until that
     * is written the request is only recorded.
     */
    private static void noteDrawing(Intent intent) {
        if (intent == null || !Intent.ACTION_VIEW.equals(intent.getAction())) {
            return;
        }
        Uri drawing = intent.getData();
        if (drawing != null) {
            Log.i(TAG, "asked to open " + drawing + " (not handled yet)");
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
