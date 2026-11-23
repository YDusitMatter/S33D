# S33D application and installer icons

The user approved the iron-walnut revision on 2026-11-12 after iterating on the
initial supplied design with the built-in image generation tool.
`s33d-artwork.png` is the resulting 1254 x 1254 transparent master: silver pipe
S3 lettering and leaves, copper 3D text, a dark iron shell with a fuller right
side, and roots connected to the silver 3 through standard pipe fittings.
The shell cracks and fine rootlets are retained. The initial supplied design,
approved generated original with provenance, and previous production assets
are retained in local maintainer backups, outside the public source inventory.
The separate converter changes only format and resolution of this selected
master; it does not redraw, crop or recolor it.

| Resource | Purpose |
| --- | --- |
| `s33d-artwork.png` | Approved iron-walnut master image |
| `s33d-seed.ico` | Windows installer executable |
| `s33d-sprouted.ico` | Application executable, shortcuts and uninstall entry |
| `s33d-sprouted.png` | Embedded application window icon |
| `s33d-seed.png` | Matching installer-role PNG input |
| `brand-manifest.json` | Revision, provenance, master/derived SHA-256 hashes and sizes |

Both roles intentionally share identical artwork. The legacy filenames keep
existing build and installer references compatible. Each ICO contains 32-bit
RGBA PNG frames at 16, 20, 24, 32, 40, 48, 64, 96, 128 and 256 pixels; the two
standalone PNGs are 512 x 512. Small icons necessarily lose fine image detail.

From the repository root, on Windows with PowerShell and System.Drawing:

```powershell
.\scripts\update-brand-icons.ps1 -Force
.\scripts\package-source.ps1 -ValidateOnly
```

The converter refuses existing derived files unless `-Force` is given and never
overwrites the master. For an isolated comparison, pass a new `-OutputDirectory`;
on the same Windows image stack, compare its manifest and asset hashes with the
checked-in files. It generates pixels and resolution data without copying image
text, EXIF, author names, source filesystem paths or account metadata. The master
was checked for text/EXIF chunks and trailing data before inclusion. Ancillary
provenance metadata is retained with the local original; the public master keeps
the approved image payload unchanged. AI-assisted editing is disclosed here and
in the hash manifest, which records both the approved original and public master.

Rebuild `s33d-app` after updating these resources. Its build script embeds the
ICO into the Windows executable and the GUI embeds the PNG separately. The
installer uses the matching ICO files from its audited stage directory. Before
distribution, verify the actual executable resources, installer icon, shortcuts
and installed window at normal Windows scaling settings. Asset generation alone
does not establish installer acceptance, signing or release approval.
