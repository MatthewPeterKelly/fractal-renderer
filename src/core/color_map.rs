/**

// Nice color map and framing here:
https://commons.wikimedia.org/wiki/File:Mandel_zoom_00_mandelbrot_set.jpg

The colors in the Mandelbrot set color map specification are given in hexadecimal format.
To convert these to RGB, we'll decode each hex color value into its respective

 red, green, and blue components. The format for each color is `index=color`,
  where the color is a decimal representation of the hexadecimal color value.
   Here are the color conversions:
   1. `6555392` in hexadecimal is `0x0063C0`.
   2. `13331232` in hexadecimal is `0xCBBA40`.
   3. `16777197` in hexadecimal is `0xFFFFFD`.
   4. `43775` in hexadecimal is `0x00AABF`.
   5. `3146289` in hexadecimal is `0x3001E1`.

   Now, let's convert these hexadecimal values to RGB:

   1. `0x0063C0`: - Red: `00` (0) - Green: `63` (99) - Blue: `C0` (192) RGB: (0, 99, 192)
   2. `0xCBBA40`: - Red: `CB` (203) - Green: `BA` (186) - Blue: `40` (64) RGB: (203, 186, 64)
   3. `0xFFFFFD`: - Red: `FF` (255) - Green: `FF` (255) - Blue: `FD` (253) RGB: (255, 255, 253)
   4. `0x00AABF`: - Red: `00` (0) - Green: `AA` (170) - Blue: `BF` (191) RGB: (0, 170, 191)
   5. `0x3001E1`: - Red: `30` (48) - Green: `01` (1) - Blue: `E1` (225) RGB: (48, 1, 225)

   To summarize:
   1. `6555392` -> RGB (0, 99, 192)
   2. `13331232` -> RGB (203, 186, 64)
   3. `16777197` -> RGB (255, 255, 253)
   4. `43775` -> RGB (0, 170, 191)
   5. `3146289` -> RGB (48, 1, 225)

 *
 */