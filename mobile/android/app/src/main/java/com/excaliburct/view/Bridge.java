package com.excaliburct.view;

import android.graphics.Bitmap;
import android.graphics.Rect;
import android.util.Log;

import com.google.android.gms.tasks.Tasks;
import com.google.mlkit.vision.common.InputImage;
import com.google.mlkit.vision.text.Text;
import com.google.mlkit.vision.text.TextRecognition;
import com.google.mlkit.vision.text.TextRecognizer;
import com.google.mlkit.vision.text.latin.TextRecognizerOptions;

/**
 * What the program asks of Android, and what Android tells the program.
 *
 * The program is the Rust library. It cannot show a system picker, a share
 * sheet or the print dialog itself, so it calls the static methods here
 * (hyperview's platform::Bridge, implemented in crates/mobile/src/android.rs),
 * and they do it on the activity's thread. What comes back later - files
 * brought in, drawings opened with the app from elsewhere - goes back through
 * the two native methods.
 *
 * Every method is safe to call from any thread.
 */
public final class Bridge {
    private static final String TAG = "ExcaliburView";

    private Bridge() {
    }

    /** Files brought in for request {@code asked}; empty when none were. */
    static native void nativeImported(long asked, String[] paths);

    /** Drawings opened with the app from somewhere else, already copied in. */
    static native void nativeOpened(String[] paths);

    // ---- asked by the program ----------------------------------------------

    /** Shows the system's picker; the files chosen are copied into {@code into}. */
    public static void importFiles(long asked, String[] mimeTypes, boolean many, String into) {
        MainActivity activity = MainActivity.current;
        if (activity == null) {
            nativeImported(asked, new String[0]);
            return;
        }
        activity.runOnUiThread(() -> activity.pickFiles(asked, mimeTypes, many, into));
    }

    /** Offers a copy of the file to the rest of the device. */
    public static void share(String path) {
        MainActivity activity = MainActivity.current;
        if (activity != null) {
            activity.runOnUiThread(() -> activity.share(path));
        }
    }

    /** Prints a PDF with Android's own printing. */
    public static void print(String path, String name) {
        MainActivity activity = MainActivity.current;
        if (activity != null) {
            activity.runOnUiThread(() -> activity.print(path, name));
        }
    }

    /** Opens a web page, or a mail link, with whatever handles it. */
    public static void openUrl(String url) {
        MainActivity activity = MainActivity.current;
        if (activity != null) {
            activity.runOnUiThread(() -> activity.openUrl(url));
        }
    }

    // ---- reading words ------------------------------------------------------

    private static TextRecognizer recogniser;

    private static synchronized TextRecognizer recogniser() {
        if (recogniser == null) {
            recogniser = TextRecognition.getClient(TextRecognizerOptions.DEFAULT_OPTIONS);
        }
        return recogniser;
    }

    /**
     * Reads the words off a greyscale picture, one byte a pixel, with ML Kit's
     * recogniser, which runs on the device and sends nothing anywhere.
     *
     * Called from one of the program's own threads, which may wait. One line
     * a word: left, top, right, bottom, confidence and the word, separated by
     * tabs. Null when it could not be read.
     */
    public static String readWords(byte[] grey, int width, int height) {
        try {
            int[] pixels = new int[width * height];
            for (int i = 0; i < pixels.length; i++) {
                int v = grey[i] & 0xff;
                pixels[i] = 0xff000000 | (v << 16) | (v << 8) | v;
            }
            Bitmap picture = Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888);
            Text read = Tasks.await(recogniser().process(InputImage.fromBitmap(picture, 0)));
            picture.recycle();
            StringBuilder out = new StringBuilder();
            for (Text.TextBlock block : read.getTextBlocks()) {
                for (Text.Line line : block.getLines()) {
                    for (Text.Element word : line.getElements()) {
                        Rect box = word.getBoundingBox();
                        if (box == null) {
                            continue;
                        }
                        String text = word.getText().replace('\t', ' ').replace('\n', ' ');
                        out.append(box.left).append('\t')
                                .append(box.top).append('\t')
                                .append(box.right).append('\t')
                                .append(box.bottom).append('\t')
                                .append(word.getConfidence()).append('\t')
                                .append(text).append('\n');
                    }
                }
            }
            return out.toString();
        } catch (Throwable e) {
            Log.w(TAG, "text recognition failed", e);
            return null;
        }
    }
}
