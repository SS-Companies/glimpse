# Recording the demo GIF + screenshot

The README ships with a synthetic SVG hero (`docs/assets/hero.svg`). It works as a placeholder, but a real recording of Glimpse on a real desktop is much more persuasive. Here is how to make one in ~5 minutes.

## What to capture

A 6–10 second loop showing the **full gesture path**, end to end:

1. Cursor hovers over text that is visibly **not selectable** — a YouTube video frame, a meme image, a game cutscene, a PDF screenshot, a chart label.
2. **Press and hold L + R mouse buttons.** The blue ring fills clockwise around the cursor.
3. The ring vanishes. **The editable preview popup appears** with the OCR'd text.
4. Press Enter (or Esc). The popup closes.
5. Switch to any text field (Notepad, Discord, address bar) and **Ctrl+V**. The text pastes.

That sequence is the entire product. Keep it under 10 seconds.

## Tool — ScreenToGif (recommended, free, MIT)

ScreenToGif is the smallest no-friction option on Windows:

1. Install: `winget install --id NickeManarin.ScreenToGif`
2. Open ScreenToGif → **Recorder**.
3. Drag the capture frame over a 1280×400 (or 1080×360) area that includes your sample text.
4. Set FPS to **30**, click Record.
5. Perform the gesture. Click Stop.
6. Editor opens. Trim to the relevant frames. **Reduce framerate to 15 fps** in the Editor → Properties tab to halve file size with no perceptible loss.
7. Save As → **`.gif`** → save to `docs/assets/demo.gif`.

Target output: < 3 MB. If yours is bigger, lower the framerate to 12 fps or crop tighter.

## Alternative — FFmpeg + your favourite video recorder

If you already record screen with OBS, Snipping Tool, or Xbox Game Bar:

```powershell
# Convert MP4 to optimised GIF
ffmpeg -i recording.mp4 -vf "fps=15,scale=1080:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse" -loop 0 docs/assets/demo.gif
```

This uses the palette-generation trick — much smaller and sharper than naive `-f gif`.

## After saving

1. Place the file at `docs/assets/demo.gif`.
2. In `README.md`, replace the hero block with:

   ```markdown
   ![Glimpse demo — hold L+R to OCR any text](docs/assets/demo.gif)
   ```

   You can keep `docs/assets/hero.svg` for fallback or use it on the GitHub Releases page.

3. Commit and push:

   ```powershell
   git add docs/assets/demo.gif README.md
   git commit -m "Add real demo GIF for README hero"
   git push
   ```

## Still screenshot for the GitHub social card

GitHub uses a separate Open Graph image for link previews. Take one good still:

1. Run `glimpse-daemon.exe`.
2. Hover over a piece of text and start the L+R hold.
3. **Halfway through**, take a Win+Shift+S screenshot of the screen with the ring half-filled.
4. Save as `docs/assets/social-card.png` (1280×640 is the GitHub recommended size).
5. Repo → Settings → Social preview → upload it.
