PDF Text Width Metrics Reference
Purpose

This document defines all metrics that influence the rendered horizontal width of a text string in a PDF.

It is intended for systems that need to:

reconstruct exact string widths

detect kerning vs manual spacing

insert new text that visually matches existing text

perform forensic typography analysis

1. Core Width Formula

For a glyph g rendered in a PDF:

glyph_visual_width =
    (glyph_width / 1000)
    * font_size
    * horizontal_scaling

Where:

Metric	Source
glyph_width	font width table
font_size	Tf operator
horizontal_scaling	Tz operator

This produces the base advance width before spacing adjustments.

2. Full String Width Formula

The rendered width of a string:

string_width =
    Σ glyph_visual_width
  + Σ kerning_adjustments
  + Σ manual_spacing_adjustments
  + character_spacing
  + word_spacing

Where each component comes from different parts of the PDF.

3. Font Metrics
3.1 Glyph Width Table

Location:

/Font
  /Widths [...]

or built-in core font metrics.

Example (Times):

A = 722
V = 722

Units:

1/1000 em

Impact:

glyph_visual_width =
    width * font_size / 1000

Importance:

PRIMARY determinant of character advance

4. Font Size

Set by operator:

Tf

Example:

/F1 12 Tf

Impact:

glyph_width_scaled =
    width * 12 / 1000

Importance:

Linear scaling of glyph widths

5. Horizontal Scaling

Operator:

Tz

Example:

100 Tz

Default:

100%

Impact:

glyph_width_scaled =
    glyph_width_scaled * (Tz / 100)

Example:

80 Tz

reduces string width to:

80%

Importance:

global width compression/expansion

6. Character Spacing

Operator:

Tc

Example:

0.5 Tc

Applied:

after every glyph

Impact:

string_width += Tc * number_of_characters

Importance:

Common in justified text.

7. Word Spacing

Operator:

Tw

Example:

5 Tw

Applied:

only to space characters

Impact:

string_width += Tw * number_of_spaces

Importance:

Common in paragraph justification.

8. Text Rendering Matrix

Operators:

Tm
Td
TD

These move the cursor.

Example:

10 0 Td

Impact:

cursor_x += 10

This affects glyph positions but not intrinsic width calculations.

Importance:

Important for layout reconstruction.

9. TJ Spacing Adjustments (Manual Kerning)

Operator:

TJ

Example:

[(A) -80 (V)] TJ

Meaning:

move next glyph closer by 80 units

Units:

1/1000 em scaled by font size

Impact:

kerning_adjustment =
    (-value / 1000) * font_size

Example:

-80 TJ

at 12pt:

-0.96 pt spacing

Importance:

explicit manual kerning

10. Font Kerning (Embedded)

Kerning may exist in font tables:

AV = -80

However:

PDF renderers may ignore font kerning unless applied explicitly.

Therefore:

Some PDFs embed kerning into TJ

Some rely on the renderer

Some bake kerning into glyph positioning

Importance:

Often not directly observable in content stream.

11. Text Matrix

Operator:

Tm

Example:

1 0 0 1 100 700 Tm

Defines:

text origin
scale
rotation

Impact:

affects glyph placement coordinates.

12. Rendering Mode

Operator:

Tr

Example:

3 Tr

Modes include:

Mode	Meaning
0	fill text
1	stroke text
2	fill + stroke
3	invisible

Invisible text can exist while still affecting spacing calculations.

13. Glyph Substitution

Some fonts replace characters with:

ligatures

Example:

fi → single glyph

Impact:

glyph count ≠ character count

Important for width reconstruction.

14. Glyph Position Drift

Glyph position is determined by:

cursor_x

Which updates after each glyph:

cursor_x += glyph_visual_width
cursor_x += spacing_adjustments

If you measure:

actual_cursor_positions

you can detect:

kerning
manual spacing
15. Width Reconstruction Algorithm

To reconstruct a string width:

for glyph in string:

    width = glyph_width / 1000
    width *= font_size
    width *= horizontal_scaling

    width += character_spacing

    if glyph == space:
        width += word_spacing

    width += manual_spacing

total_width += width
16. Kerning Detection Strategy

Compute:

expected_width =
    Σ glyph_widths

Measure:

actual_width =
    last_glyph_x - first_glyph_x

Difference:

delta = actual_width - expected_width

Interpretation:

Delta	Meaning
0	no kerning
negative	tighter spacing
positive	expanded spacing
17. Metrics Summary Table
Metric	Operator	Impact
Glyph width	font table	base advance
Font size	Tf	linear scaling
Horizontal scaling	Tz	width compression
Character spacing	Tc	added per glyph
Word spacing	Tw	added per space
Manual kerning	TJ	explicit glyph spacing
Cursor movement	Td/Tm	affects layout
Font kerning	font table	optional renderer behavior
18. Minimal Metrics Needed for Accurate Width

To reconstruct width you must know:

font
glyph widths
font size
horizontal scaling
character spacing
word spacing
manual TJ spacing

Everything else influences position, not intrinsic width.

19. Metrics Needed for Kerning Detection

You need:

glyph positions
glyph identities
font width table
font size

Then compare:

expected vs actual width
20. Practical Implementation

A kerning detection system should:

Extract glyphs

Record glyph x positions

Compute expected widths

Compare actual spacing

Flag deviations
