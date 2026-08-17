from pathlib import Path

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text()

count = text.count("Ordering::Relaxed")
if count != 8:
    raise SystemExit(f"expected 8 Task 27 ownership Relaxed orderings, found {count}")

text = text.replace("Ordering::Relaxed", "Ordering::SeqCst")
path.write_text(text)
