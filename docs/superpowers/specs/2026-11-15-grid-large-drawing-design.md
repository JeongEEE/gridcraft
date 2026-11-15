# Large DrawingML import

The owner requested that the affected workbook open and remain editable, with the fix on
`my-custom`. Its drawing has 37,000 small picture anchors and exceeds the whole-tree XML
node limit. The XML is well formed. Skipping it also loses pictures on save.

Use the existing streaming XML parser to consume one anchor at a time. Keep the existing
per-tree depth/node limits and package byte limits; bound the total anchors to 100,000 per
drawing. Import is atomic for malformed or over-limit drawings: discard that drawing's
partially imported objects and report a warning. Other sheets and prior drawings survive.

Resolve drawing relationships by id through a hash map. Read each referenced media part
once per drawing and share its `Arc<Vec<u8>>` between independent model objects. Preserve
all objects, their order, geometry and bytes; do not deduplicate or remove user objects.

The UI texture cache identifies the immutable shared media allocation rather than the
picture object id. A retained cache entry holds a weak allocation reference to prevent pointer reuse without
retaining deleted encoded media. Enable the existing image crate's GIF decoder so the
workbook's embedded GIF displays; animation remains a first-frame preview.
Retain document isolation, size-aware decoding and the existing cache budgets.

Raising the global node limit would reintroduce unbounded XML tree memory. Removing
duplicate objects would change the user's document. Neither approach meets the request.

Verify synthetic >1M-node import/edit/save/reopen, shared media/textures, malformed input,
the object bound and the real workbook through the engine and native UI. Keep personal
workbook bytes and screenshots outside git. This change does not promise general Excel
feature parity or publish a new release.
