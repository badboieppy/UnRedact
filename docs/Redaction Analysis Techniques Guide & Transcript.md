# **Redaction Analysis Techniques Guide**

**Source:** Video transcripts from Epstein files analysis series (5 videos, January-February 2026\) 

[https://www.youtube.com/playlist?list=PLIYSq4q5r0iR2afD53jIX0u8zdJcyUFnx](https://www.youtube.com/playlist?list=PLIYSq4q5r0iR2afD53jIX0u8zdJcyUFnx)

**Purpose:** Extract techniques for identifying names beneath PDF redaction boxes

---

## **Core Methodology**

The analysis combines **font forensics**, **pixel artifact detection**, **width-matching**, and **cross-document correlation** to identify redacted names. The process requires design software (Affinity Designer 2, Photoshop, or similar) with text overlay and pixel analysis capabilities.

---

## **1\. Font Identification & Width Matching**

### **Technique**

Match the exact font and size used in the document to calculate precise character widths for candidate names.

### **Process**

**Step 1: Identify font and size**

* Examine unredacted text in same document  
* Most common fonts in Epstein files:  
  * Times New Roman 12pt (legal documents)  
  * Calibri 11pt or 12pt (emails, modern documents)  
  * Segoe UI 10.15pt (email headers)  
  * Arial 10.15pt (email body alternate)

**Step 2: Create text overlay**

* Create text box in design software  
* Set to identified font/size  
* Type candidate name over redaction box  
* Align carefully with surrounding text

**Step 3: Verify alignment**

* Name should fit precisely within redaction boundaries  
* Should not push punctuation/spacing beyond original position  
* Test alternate fonts if alignment imperfect (e.g., Cambria vs Times New Roman)

### **Example from Transcript**

"This font is Times New Roman. And you can see all I did was created   
a text box, size 12 Times New Roman font. And I just kind of laid it   
over there, and it lines up pretty perfectly."

Testing alternate font: "Let's try Cambria, which is another common   
serif font. Cambria there no longer lines up. You can see it. It's   
completely off."

### **Precision Method: Using Guides**

**Setup vertical guides:**

1. Enable ruler (View → Show Rulers)  
2. Click and drag from ruler to create guides  
3. Align guide with known text box edges  
4. Enable snapping (View → Snapping → Snap to Guides)  
5. Text boxes "click into place" when aligned

**Benefits:**

* Ensures consistent starting position when left edge unclear  
* Guide turns green when properly snapped  
* Works even when text doesn't have clear boundaries on both sides

### **Critical Settings Adjustments**

**Kerning (letter spacing):**

* **Auto vs Zero:** Documents vary in kerning settings  
* Test by checking if punctuation alignment changes  
* Example: Period after "R" may be farther/closer with different kerning  
* **Fix:** Character window → Set kerning to 0% or Auto depending on document  
* **Warning:** Wrong kerning can make correct names appear incorrect

**Tracking (overall spacing):**

* Adjusts space between all letters uniformly  
* Typically 1-5% variation between documents  
* Test on known unredacted names first

**Ligatures:**

* When enabled, certain letter pairs "hold hands" (e.g., "ff")  
* Collapses character width slightly  
* **Check:** Look for gaps between letters like "ff" in unredacted text  
* If no gaps visible, ligatures likely disabled  
* **Warning:** Ligatures ON in document but OFF in your text \= incorrect width

---

## **2\. Pixel Artifact Analysis**

### **What Are Pixel Artifacts?**

Redaction boxes sometimes fail to completely cover text, leaving traces at edges:

* Partial letterforms visible at box borders  
* Anti-aliasing creates gray/colored pixels revealing character shapes  
* Serifs, descenders, ascenders extend beyond redaction boundaries

### **Artifact Categories**

**Edge artifacts:**

* Occur along left/right borders of redaction box  
* Indicate first/last characters  
* Most reliable for identification

**Corner artifacts:**

* Appear in all four corners of many redaction boxes  
* Often caused by box itself, not underlying text  
* **Warning:** Don't overinterpret corner pixels \- common to all boxes

**Descender/ascender artifacts:**

* Letters with tails below baseline: g, y, p, q, j  
* Letters extending above x-height: h, k, l, b, d, t  
* Example: "g-tail technique" identifies Gmail addresses by descender of lowercase 'g'

### **Detection Techniques**

#### **Technique 1: Visual Inspection**

* Zoom to 400-800% magnification  
* Examine all four edges of redaction box  
* Look for:  
  * Curved shapes (C, G, O, Q)  
  * Vertical stems (I, l, t, d, b)  
  * Serifs (projections on letters in serif fonts)

#### **Technique 2: Color Picker Tool**

* Use eyedropper tool in design software  
* Hover over suspicious pixels  
* **RGB mode:** Shows red/green/blue values when colors change  
* **Lab mode (recommended):** Shows lightness value (L)  
  * Darker \= lower L value  
  * Lighter \= higher L value  
  * More precise for subtle variations

**How to use:**

1\. Change document color profile to Lab/L\*a\*b 16  
2\. Select eyedropper/color picker  
3\. Left-click and hold while dragging over pixels  
4\. Watch L value change in display  
5\. Darker areas \= possible letter remnants

#### **Technique 3: Curves Adjustment Layer**

* Add adjustment layer over document (non-destructive)  
* Access: Layer → New Adjustment → Curves  
* Manipulate light/dark values to enhance contrast

**Process:**

1. Add curves adjustment layer  
2. Increase contrast by:  
   * Moving bottom-left point up (lightens darks)  
   * Moving top-right point down (darkens lights)  
   * Adjusting midpoint for specific gray values  
3. Artifacts become more visible  
4. **Warning:** Can create illusion of shapes that don't exist \- use cautiously

#### **Technique 4: Flood Fill Color Mapping**

* Most precise technique for visualizing pixel variations  
* Creates "color map" showing exactly where pixels differ

**Process:**

1. Duplicate layer (non-destructive editing)  
2. Rasterize layer (convert to pixels if vector)  
3. Select flood fill tool  
4. Set tolerance to 0% (only exact color matches)  
5. Set fill color lightness mode  
6. Click darkest pixel → fill with color  
7. Slightly lighten fill color  
8. Click next-darkest pixels → fill  
9. Repeat, progressively lightening, until all artifacts mapped

**Benefits:**

* Shows precise boundaries of pixel variations  
* Reveals subtle differences invisible to naked eye  
* Can compare artifact patterns to unredacted letters

**Example from transcript:**

"We can make a color map of the artifacts. So, we can start with that.   
And what we're going to do is just kind of start filling in dark to   
light. \[...\] And you can see when we look at that now, if we zoom out,   
you can see that there's actually a pretty significant difference."

### **Interpreting Specific Artifacts**

**Round first letters (C, G, O, Q):**

* Curved artifact at left edge of redaction  
* Width of dark region indicates letter  
* Example: "G" has narrow curve, "O" has wider curve

**Letters with horizontal serifs (T, I, L):**

* Small pixel extending perpendicular to main stroke  
* Top serifs \= capital letters or tall lowercase  
* Bottom serifs \= baseline alignment

**Descenders below baseline (g, y, p, q, j):**

* Pixels extending below redaction bottom edge  
* "g-tail technique": Lowercase 'g' in Gmail addresses  
* Example: `jeevacation@gmail.com` shows 'g' tail below redaction

**Specific letter identification:**

* **A:** Pixel artifact appears at specific height from tail \- higher than D or U  
* **D:** Curved but tail artifact lower than A  
* **R:** Vertical stem \+ curved upper portion  
* **S:** Curved top and bottom (double curve signature)

---

## **3\. Character Width Analysis (Min/Max Length)**

### **Purpose**

Determine absolute minimum and maximum number of characters in redacted name.

### **Process**

**Maximum character count:**

1. Type narrowest letters repeatedly: `IIIIIIIIII`  
2. Capital at start (formatting requirement)  
3. Include one space (first name \+ last name)  
4. Fill until text extends beyond redaction  
5. Count characters

**Minimum character count:**

1. Type widest letters repeatedly: `MMMMMMM`  
2. Capital at start  
3. Include one space  
4. Fill until text barely fits  
5. Count characters

**Refinement using known constraints:**

* If first letter known (e.g., artifact shows round letter):  
  * Use narrowest round letter (C) for max count  
  * Use widest round letter (Q) for min count  
* If last letter known:  
  * Substitute appropriate narrow/wide variant

### **Example from Transcript**

Chief Engineer redaction analysis:

"We know that their first name likely begins with a round letter. So,   
this little artifact here means their name probably starts with a C, a   
G, an O, or a Q. Then we have over here the last letter is going to be   
a K, a T, an X, or a Z based on these couple of artifacts."

Maximum: Using C (narrow round) \+ I's \+ T (narrow ending) \= 26 characters  
Minimum: Using Q (wide round) \+ M's \+ K (wide ending) \= 9 characters

### **Using Character Counter**

* Instead of manual counting: copy text → paste into online character counter  
* Tools: Grammarly character counter, any online tool  
* Counts characters/words/sentences accurately  
* Faster and eliminates human counting errors

---

## **4\. Cross-Document Correlation**

### **Principle**

Same name may appear redacted in one document but unredacted in another, or redacted differently across documents.

### **Techniques**

**Same person, multiple formats:**

* Last name only (leading caps): `Kellen`  
* First \+ last (leading caps): `Sarah Kellen`  
* First \+ last (all caps): `SARAH KELLEN`  
* Matching across formats increases confidence

**Redaction consistency analysis:**

* Older redaction under newer redaction creates pixel line  
* Darkness shifts where previous redaction ends  
* Indicates original redactor only hid last name, later redactor covered full name

**Example from transcript:**

"This one over here is a really interesting pixel artifact. And this   
gets into how I was able to figure out the other one the other set of   
redactions, the longer ones, because you'll notice there's this pixel   
here and then it changes and becomes darker for the rest of the box.   
So it's lighter here, darker on this side. \[...\] That means that I   
would interpret that as to mean this was a redaction a previous   
redaction that is under the bigger redaction."

### **Multi-Document Verification**

* Test candidate name in multiple documents with different fonts  
* Same name fitting in Times New Roman AND Calibri \= strong corroboration  
* Pixel artifacts across documents provide independent verification

---

## **5\. Context & Corroboration**

### **Document Context Analysis**

**Read surrounding unredacted content:**

* Who is being discussed?  
* What role did they play?  
* Location references (e.g., "approached in Florida")  
* Time period references

**Pattern recognition:**

* Grouped redactions likely related people  
* Example: If unredacted names include "Maxwell, Brunell, Wexner," redacted names likely other associates  
* Format consistency: If unredacted uses last names only, redacted probably follows

**Missing information flags:**

* Documents with suspicious omissions  
* Example: FBI investigation summary missing 2008 plea deal bullet point

### **External Corroboration**

**Biographical verification:**

* Does candidate match role description?  
* Geographic location alignment (Florida resident for Florida subpoena)  
* Time period alignment (person's documented whereabouts)

**Social media verification:**

* Instagram/Twitter timeline for location during relevant dates  
* Public statements or travel  
* **Warning:** Absence of evidence ≠ evidence of absence

**Database cross-reference:**

* Check if name appears in:  
  * Epstein's black book  
  * Flight logs  
  * Other victim testimony  
  * Financial records  
  * Entity registries

### **Triangulation Example**

Sarah Kellen verification across multiple sources:

1\. Width match in Times New Roman 12pt (warrant)  
2\. Pixel artifacts show "S" and "K" serifs  
3\. Width match in Calibri 12pt (different document)  
4\. Name appears unredacted in third document  
5\. Known Florida resident (subpoena location match)  
6\. Appears in multiple victim statements

Confidence: Very high

---

## **6\. Methodological Principles**

### **The Scientific Mindset**

**Goal:** Get it right, not be right

* Willingness to be wrong is fundamental  
* Every finding is provisional  
* Update conclusions when new evidence appears

**Confirmation bias mitigation:**

1. Test multiple candidate names even after finding match  
2. Actively seek disconfirming evidence  
3. Document names that DON'T fit  
4. Re-examine every assumption if one proves wrong

**From transcript:**

"I'm not trying to be right. I'm trying to get it right. And in order   
to get it right, you have to be willing to be wrong. It is not only   
not a bad thing to be wrong, it is just a fundamental part of the   
process."

### **Hold Ideas Loosely**

**What this means:**

* Never become attached to preliminary conclusions  
* Treat matches as hypotheses requiring verification  
* If one conclusion wrong, re-examine everything built upon it

**Example from videos:**

* Initial candidate fit perfectly, had explosive implications  
* Geographic evidence contradicted (Instagram showed person abroad)  
* Entire theory revised based on new evidence  
* Replacement candidate (Haley Robson) had better corroboration

### **Avoiding Nudging**

**The risk:**

* Unconsciously shifting text boxes to make names fit  
* Overlooking artifacts that contradict preferred answer  
* Seeing patterns that confirm expectations

**Mitigation strategies:**

1. Use guides/snapping to lock positions  
2. Test alignment against multiple known reference points  
3. Have second person verify alignment independently  
4. Document why alternatives DON'T fit

---

## **7\. Specialized Techniques**

### **Analyzing Longer Redactions**

**Challenge:** Multiple names in single redaction block

* Example: List of 7 names in 9 redaction boxes

**Technique:**

1. Identify breaks between names (pixel patterns, spacing gaps)  
2. Note which names are duplicated (format consistency)  
3. Cross-reference with unredacted names in same list  
4. Use context (e.g., "among those served included..." \= incomplete list)

**Example:**

Email lists 10 co-conspirators:  
\- 3 unredacted: Brunell, Maxwell, Wexner  
\- 7 redacted across 9 boxes  
\- 2 duplicates expected

Process: Match known duplicates first (Sarah Kellen appears 3x),   
then work through unique redactions

### **Box Height Analysis**

**Observation:** Some redaction boxes are taller than others

**Possible interpretations:**

* **Descenders:** Letters with tails below baseline (g, y, p, q)  
* **Ascenders:** Tall letters (h, k, l, b, d, t, f)  
* **Careless redaction:** DOJ redactors working quickly, inconsistent box sizes

**Usage:**

* Suggestive evidence only  
* Don't draw firm conclusions from height alone  
* **Example:** Taller boxes for "Nadia Marcinkova" (descender in 'k') vs shorter for "Ross"

---

## **8\. Common Pitfalls & Warnings**

### **Settings Mismatches**

* **Kerning differences** between documents can make correct names appear wrong  
* **Ligatures ON/OFF** changes character width significantly  
* **Always verify settings** against unredacted text in same document

### **Over-Interpreting Pixels**

* **Corner artifacts** appear on all boxes (not meaningful)  
* **Compression artifacts** from scanning/copying create false patterns  
* **Document-to-document variation** in same font renders differently

### **Confirmation Bias**

* Finding name that fits explosive theory ≠ verification  
* Must seek disconfirming evidence  
* Geographic, temporal, contextual contradictions override width match

### **Privacy & Ethics**

* Many "co-conspirators" were victims themselves  
* Inclusion in files ≠ guilt  
* **Never send harassment** to identified individuals  
* Prioritize victim privacy over public curiosity

---

## **9\. Verification Hierarchy**

### **Confidence Levels**

**Very High (0.9-1.0):**

* Width match in multiple fonts  
* Pixel artifacts corroborate specific letters  
* Name appears unredacted in related documents  
* Geographic/temporal evidence aligns  
* Appears in victim testimony or financial records

**High (0.7-0.9):**

* Width match in single font  
* Some pixel artifacts support  
* Contextual plausibility  
* Cross-document correlation

**Medium (0.5-0.7):**

* Width match only  
* Limited contextual support  
* No contradicting evidence

**Low (0.3-0.5):**

* Width match with caveats  
* Weak contextual fit  
* Requires additional verification

**Speculative (\<0.3):**

* Fits box dimensions but lacks other evidence  
* Should not be stated as conclusion

---

## **10\. Practical Workflow**

### **Standard Analysis Process**

**1\. Document preparation:**

* Identify font/size from unredacted text  
* Configure design software settings (kerning, ligatures, tracking)  
* Set up guides for alignment reference

**2\. Initial assessment:**

* Examine pixel artifacts (all four edges)  
* Note any visible letter shapes  
* Determine min/max character count  
* Identify constraints (round first letter, specific last letter, etc.)

**3\. Candidate generation:**

* Query name databases (black book, entity registries, victim lists)  
* Filter by character count constraints  
* Filter by first/last letter constraints  
* Prioritize by mention frequency in documents

**4\. Testing:**

* Create text overlay for each candidate  
* Check width alignment  
* Verify pixel artifacts match  
* Test in multiple documents if available

**5\. Verification:**

* Geographic alignment check  
* Temporal alignment check  
* Role plausibility check  
* Cross-reference other documents  
* Document why alternatives don't fit

**6\. Documentation:**

* Record confidence level  
* Note corroborating evidence  
* List tested alternatives that failed  
* Identify what would increase/decrease confidence

---

## **11\. Tool Recommendations**

### **Design Software**

* **Affinity Designer 2** (used in videos, now free via Canva)  
* **Adobe Photoshop** (industry standard)  
* **GIMP** (free alternative)

**Required features:**

* Text overlay capabilities  
* Precise font rendering  
* Guides and snapping  
* Eyedropper/color picker  
* Adjustment layers (curves)  
* Flood fill with tolerance control

### **Analysis Tools**

* **Text Width API:** [https://text-width-api-703926648457.us-central1.run.app/](https://text-width-api-703926648457.us-central1.run.app/)  
  * Calculates pixel-width of text strings  
  * Supports multiple fonts  
  * Enables batch candidate testing  
* **Character counters:** Any online tool for accurate length counting

### **Reference Materials**

* Epstein's black book (digitized in Google Sheets)  
* Entity registries (1,536+ persons in Eric's database)  
* Flight logs  
* Financial transaction records  
* Congressional Reading Guide (90 high-priority documents)

---

## **12\. Example Walkthrough: Sarah Kellen**

### **Step 1: Initial Discovery (Warrant Document)**

* **Font identified:** Times New Roman 12pt  
* **Artifact discovered:** Missed spot revealed "S" as first letter  
* **Format:** First and last name, all caps

### **Step 2: Width Matching**

* Created text box, Times New Roman 12pt  
* Typed "SARAH KELLEN"  
* Perfect alignment within redaction boundaries  
* Tested alternatives (e.g., "Steve Bannon") \- pushed punctuation, didn't fit

### **Step 3: Pixel Verification**

* Artifacts at left edge align with "K" serifs  
* Artifact at right edge aligns with "N" serif  
* Corner pixels consistent with letterforms

### **Step 4: Cross-Document Correlation**

**Found in 3 formats across 3 documents:**

1. Warrant: `SARAH KELLEN` (all caps, Times New Roman 12pt)  
2. Email: `Kellen` (last name only, leading caps, Times New Roman 12pt)  
3. FBI summary: `Sarah Kellen` (first \+ last, leading caps, Calibri 12pt)

### **Step 5: External Verification**

* Known associate of Epstein  
* Florida resident (matches subpoena location)  
* Appears in multiple other documents  
* Listed in black book  
* Mentioned in victim testimony

### **Confidence: Very High (0.95)**

---

## **13\. Key Quotes from Analyst**

On methodology:

"This is pretty straightforward thing to be figuring out what the font is, what the size is at least for the electronics and not it doesn't work as well on things that have been like physically scanned from the start and have like warping, but it it does work for a lot of these digital stuff."

On verification importance:

"Claims must be evidence-based, grounded in reality, and falsifiable. \[...\] We need to be able to put ideas out there and talk with other people and approach this with a level of curiosity and humility to be able to accept what other people are saying when they think that we are missing something."

On ethical responsibility:

"These are real people. These are people who have for whatever reason, you know, they they've been dismissed, at least for now, they are innocent until proven guilty. And so please do not send hate their way."

On limitations:

"I cannot emphasize enough that I am not a font expert \[...\] I'm already pushing up against the edge of what I know here. So, if we're going to be automating and expanding this at the rate that we want to, we're going to need you guys."

---

## **14\. Resources for Further Learning**

### **Community**

* Discord server (200+ active contributors as of January 2026\)  
* Reddit r/Epstein community  
* Collaborative Google Sheets tracking progress

### **Documentation**

* Community Reference Guide (tracking 2.7M pages, 60K completed)  
* Redactions Analysis spreadsheet (character widths, font specifications)  
* Resource catalog (tools, articles, databases)

### **Technical References**

* Font specimen books (character width charts)  
* Typography fundamentals (kerning, ligatures, tracking)  
* PDF forensics research papers  
* Image processing techniques

---

## **Summary**

Redaction analysis combines precision font-matching, pixel-level artifact detection, systematic candidate testing, and multi-source corroboration. Success requires:

1. **Technical precision:** Exact font replication, careful settings management  
2. **Visual analysis:** Pixel artifact interpretation, pattern recognition  
3. **Scientific rigor:** Testing alternatives, seeking disconfirming evidence  
4. **Ethical awareness:** Protecting victims, avoiding speculation  
5. **Iterative refinement:** Updating conclusions when new evidence emerges

The techniques documented here emerged from grassroots community analysis following January 2026 DOJ document release. Methods continue to evolve as contributors refine approaches and discover new patterns.

---

**Document prepared:** February 2026  
**Based on:** 4 video transcripts totaling \~28,000 words  
**Techniques extracted:** 14 major categories covering font forensics, pixel analysis, verification frameworks, and methodological principles

