//! Panel detection v6 — Comic Trim–style gutter BSP (from Comic Trim v4.20 APK).
//!
//! Pipeline (pure Rust, no OpenCV/ML):
//! 1. Downsample (~1200–1600 max side) for speed; normalize rects to 0..1.
//! 2. Sample ~12 margin points; estimate paper luma (cluster).
//! 3. Binarize: pixel is gutter if near paper-white OR near black (within tol).
//! 4. Recursive H/V empty-band BSP: trim empty top/left, find first strong empty
//!    row-band or col-band, split, recurse. Leaf area > page/70 → panel.
//! 5. Extreme gutter ratio or weak splits → whole page (or clean full-width rows).
//!
//! Why not contour/CC (v5): contours crop figures *inside* framed panels;
//! gutter BSP only cuts empty bands *between* frames (Italian 2-up grids).

include!("detect_part1.inc.rs");
include!("detect_part2.inc.rs");
include!("detect_part3.inc.rs");
