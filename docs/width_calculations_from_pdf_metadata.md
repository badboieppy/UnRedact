1. Base Glyph Width

Every character has a width stored in the font width table.

Example (Times-Roman):

E = 611
P = 556
S = 556
T = 611

These are in 1/1000 em units.

Formula
glyph_width_pt =
    (width / 1000) * font_size

Example (12pt font):

E width = (611 / 1000) * 12
        = 7.332 pt
2. Horizontal Scaling (Tz)

Horizontal scaling compresses or stretches text.

Default:

100 Tz
Formula
scaled_glyph_width =
    glyph_width_pt * (Tz / 100)

Example:

glyph_width = 7.332 pt
Tz = 80

scaled_width = 7.332 * 0.8
             = 5.8656 pt
3. Character Spacing (Tc)

Character spacing is added after every glyph.

Formula
char_spacing_total =
    Tc * number_of_characters

Example:

Tc = 0.2 pt
chars = 7

char_spacing_total = 1.4 pt

Note:

Spacing is applied between glyphs, so often:

Tc * (n - 1)

depending on renderer behavior.

4. Word Spacing (Tw)

Word spacing is added only for spaces.

Formula
word_spacing_total =
    Tw * number_of_spaces

Example:

Tw = 4 pt
spaces = 2

word_spacing_total = 8 pt
5. Manual Spacing (TJ operator)

Example:

[(A) -80 (V)] TJ

PDF units are 1/1000 em scaled by font size.

Formula
manual_spacing_pt =
    (-value / 1000) * font_size

Example:

value = -80
font_size = 12

spacing = -(-80 / 1000) * 12
        = 0.96 pt tighter

Note: the sign is inverted because TJ values move the cursor.

6. Text Matrix Scaling

The text matrix can scale width.

Example:

Tm = [a b c d e f]

Horizontal scale = a.

Formula
glyph_width_scaled =
    glyph_width * a

Usually:

a = 1

unless text is transformed.

7. Putting It All Together

Full width formula:

string_width =
    Σ (glyph_width / 1000 * font_size * horizontal_scaling)
  + Tc * num_chars
  + Tw * num_spaces
  + Σ TJ_adjustments
8. Practical Example

String:

EPSTEIN

Font size:

12 pt

Widths (Times):

E = 611
P = 556
S = 556
T = 611
E = 611
I = 278
N = 722
Step 1 — Base widths
(611+556+556+611+611+278+722)/1000 * 12
= 3,945 / 1000 * 12
= 47.34 pt
Step 2 — Character spacing
Tc = 0.1
chars = 7

0.1 * 7 = 0.7 pt
Step 3 — TJ spacing

Example:

-30
(-(-30)/1000)*12
= 0.36 pt tighter
Final width
47.34 + 0.7 - 0.36
= 47.68 pt
9. Programmatic Algorithm

Pseudo-code:

width = 0

for glyph in string:

    glyph_width = font_width[glyph]

    base =
        glyph_width / 1000 *
        font_size *
        horizontal_scaling

    width += base

    width += Tc

    if glyph == " ":
        width += Tw

    width += TJ_adjustment[glyph]

return width
10. Metrics Needed for Accurate Width Calculation

You must extract:

font width table
font size (Tf)
horizontal scaling (Tz)
character spacing (Tc)
word spacing (Tw)
TJ adjustments
text matrix scale
11. What Matters Most

In practice:

Metric	Typical Impact
font width	~95% of width
font size	linear scale
TJ spacing	fine typography
Tc	paragraph justification
Tw	justified spacing
