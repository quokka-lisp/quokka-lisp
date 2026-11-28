//! Saving and loading a world.
//!
//! An *image* is the whole heap written to bytes and read back: every symbol,
//! every function, every half-finished computation. Loading one is how Quokka
//! starts, as it was how a Lisp machine started and how Smalltalk still
//! starts. Booting means loading an image, not running a build.
//!
//! # Why this module is almost empty
//!
//! A `save world` is usually the hardest thing in a runtime, because the
//! obvious representation of a heap is full of raw pointers — addresses that
//! meant something only in the process that wrote them. Writing those to a
//! file saves numbers that will be wrong on the next run, so the file format
//! has to be a *different* representation, and loading becomes a graph
//! traversal that rebuilds every object and patches every pointer.
//!
//! Quokka does none of that, because of a decision made in phase 0 for an
//! apparently unrelated reason. Rule 3: **heap references are offsets, never
//! raw pointers.** A `Value` that names a cons cell holds the *word index* of
//! that cell within the heap's `Vec<u64>` — not an address. The rule was
//! written so that wasm32 and 64-bit native could run the same code, and so
//! that a copying collector could move the whole heap without anything
//! noticing.
//!
//! The consequence, which falls out for free: **the heap is already
//! position-independent.** Index 4,312 means the same thing in any process
//! that has the same words. So saving is `words` to little-endian bytes, and
//! loading is bytes back to `words`. No traversal, no pointer patching, no
//! second representation to keep in step with the first.
//!
//! This is what the design meant by "an image loads by relocation or straight
//! `mmap`", and it is worth noticing how much work a rule chosen for a
//! different reason turned out to save. The GC Handbook (Jones, Hosking and
//! Moss, 2012) §11 surveys what the alternatives cost.
//!
//! # And why it can save a computation in mid-air
//!
//! The second decision that pays off here is rule 4: the VM must be able to
//! yield, so **execution state lives in the heap, never on the Rust call
//! stack.** The evaluator's registers, the frame list, the bytecode VM's
//! program counter and operand stack are all either heap objects or numbers
//! in the image header's *scalars* — the nineteen fields listed below.
//!
//! So there is no such thing as "the part of the computation that is in Rust
//! and cannot be saved". An image written between two instructions reloads
//! between those two instructions, and the computation continues. That is
//! phase 1's gate, and it is not a feature this module adds — it is a feature
//! this module *cannot avoid*, given how the VM was already built.
//!
//! # What is not here yet
//!
//! Compression, `mmap` rather than a copy, coalescing equal literals (#26),
//! and any record of what happened *between* two images — the change log of
//! #24. The one piece of that which could not wait is the sequence number in
//! the header, because a field cannot be added to images that already exist.

use crate::heap::{Heap, Inconsistency, ObjectType, Scalars};
use crate::value::Value;

/// `"QUOKKA\0\0"` read as a little-endian `u64`.
///
/// A magic number so that a file which is not an image is refused as one
/// rather than loaded as a heap full of nonsense. Eight bytes, so it is also
/// the alignment the rest of the format assumes.
pub const MAGIC: u64 = u64::from_le_bytes(*b"QUOKKA\0\0");

/// The format this build writes, and the only one it reads.
///
/// There is no reader for an older format, and there will not be one: an
/// image moves to a new format **through its source** (#145, decided 28 Nov
/// 2026). A world writes out its definitions, whose text it keeps (#55),
/// and its data (`make-load-form`), and a fresh world of the new format
/// reads them in. Readers for old formats in Rust would grow for ever, one
/// for every layout the heap has had.
///
/// That path is not built yet. Until it is, a change of this number loses
/// a stored world, and the start page is to say so; which is why the
/// changes of format already known (parked processes, per-process value
/// cells, packed arrays) are made together, before anyone keeps a world.
///
/// **Version 2** added the `Request` register (#35), two days after version 1
/// was fixed — which is the honest measure of how stable a format is before
/// anything depends on it, and why #34 says the format is stable for phase 1
/// rather than forever.
///
/// **Version 3** packs constant strings eight Latin-1 characters to a word
/// (#44), a new object type an older reader would refuse anyway. **Version 4**
/// gives a hash table a fourth slot, saying whether a key was hashed by its
/// address (#64). **Version 5**: a structure is an `Instance`, no longer a
/// vector, and markers have a type of their own (#73). **Version 6**: an
/// expander takes the form and the environment (#76); an older world's
/// compiler would call the new rewrite's expanders the old way.
///
/// **Version 7** (#98, #99): the checksum is FNV-1a with the prime spelt
/// correctly; hash tables place their keys by a finalised hash, so a table
/// in an older image has its entries in the wrong buckets; "no unwind
/// record" is `u64::MAX` on every target; and the header says how many
/// primitives and opcodes the image was built against.
///
/// **Version 8** (#124): standard and funcallable instances, two new object
/// types.
///
/// **Version 9** (#130): a call frame keeps the call that made it, its
/// arguments, their count and the callee, below a seven-word header; a world
/// saved mid-computation has frames in the old shape.
///
/// **Version 10** (#34): processes, a new object type, and a register for
/// the running one, appended to the scalars.
///
/// **Version 11** (#34): a process has a slot more, the request to end it.
pub const FORMAT_VERSION: u64 = 11;

/// Words before the heap's own: magic, version, length, checksum, scalars,
/// and how much machine the image needs.
pub const HEADER_WORDS: usize = 4 + Scalars::WORDS + MACHINE_WORDS;

/// The primitive count and the opcode count.
const MACHINE_WORDS: usize = 2;

/// Why an image could not be loaded.
///
/// Every variant is a thing that happens to real files — truncated downloads,
/// a path typo, an image from an older build — rather than a defensive
/// gesture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// Not long enough to be an image at all.
    TooShort {
        /// How many bytes there were.
        bytes: usize,
    },
    /// The first eight bytes are not [`MAGIC`].
    NotAnImage {
        /// What was there instead.
        found: u64,
    },
    /// Written by a different version of Quokka.
    WrongVersion {
        /// The version in the file.
        found: u64,
        /// The version this build writes.
        expected: u64,
    },
    /// Not a whole number of 64-bit words, which every image is.
    NotWholeWords {
        /// How many bytes there were.
        bytes: usize,
    },
    /// Built against a machine with more primitives or opcodes than this one.
    NeedsNewerMachine {
        /// How many primitives the image's code may name.
        primitives: u64,
        /// How many opcodes.
        opcodes: u64,
    },
    /// A compiled function in the image that the bytecode verifier refused.
    BadCode(String),
    /// The header's word count does not match the bytes that follow.
    WrongLength {
        /// What the header claimed.
        claimed: u64,
        /// What was actually there.
        found: usize,
    },
    /// The checksum does not match: the file changed after it was written.
    Corrupt {
        /// The checksum in the header.
        claimed: u64,
        /// The checksum of the bytes as read.
        found: u64,
    },
    /// The words loaded, but they are not a consistent heap.
    Inconsistent(crate::heap::Inconsistency),
}

impl core::fmt::Display for ImageError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ImageError::TooShort { bytes } => {
                write!(f, "{bytes} bytes is too short to be an image")
            }
            ImageError::NotAnImage { .. } => write!(f, "not a Quokka image"),
            ImageError::NotWholeWords { bytes } => write!(
                f,
                "{bytes} bytes is not a whole number of words: truncated, or not an image"
            ),
            ImageError::NeedsNewerMachine {
                primitives,
                opcodes,
            } => write!(
                f,
                "the image was built for a newer Quokka ({primitives} primitives, \
                 {opcodes} opcodes): run it with that one, or rebuild it from source"
            ),
            ImageError::BadCode(why) => {
                write!(
                    f,
                    "the image holds a compiled function that is malformed: {why}"
                )
            }
            ImageError::WrongVersion { found, expected } => write!(
                f,
                "image format {found}, but this build reads {expected} — \
                 rebuild the image from source"
            ),
            ImageError::WrongLength { claimed, found } => write!(
                f,
                "the image says {claimed} words and has {found}: truncated"
            ),
            ImageError::Corrupt { .. } => {
                write!(f, "the image's checksum does not match: it is damaged")
            }
            ImageError::Inconsistent(i) => {
                write!(f, "the image loaded but is not a consistent heap: {i:?}")
            }
        }
    }
}

/// FNV-1a, 64-bit, over a run of words.
///
/// Not a cryptographic hash and not trying to be: it catches a truncated
/// download, a half-written file and a flipped bit, which are the failures
/// that actually happen. Something that resists a *deliberate* edit would
/// have to be, and an image you did not write is code you are about to run
/// anyway — the design says as much under "Credentials stay out of the
/// image". Fowler, Noll and Vo's function is four lines and has no table.
///
/// Over an iterator, so that neither saving nor loading has to copy the heap
/// just to add it up: on wasm32 a second copy of the world is memory the
/// page may not have (#99). The prime is 2^40 + 0x1b3; through format 6 it
/// was mistyped as 2^44 + 0x1b3, which checked just as well and matched no
/// other FNV-1a (#98).
fn checksum(words: impl Iterator<Item = u64>) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for word in words {
        for byte in word.to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// How much machine an image needs: how many primitives, how many opcodes.
///
/// Compiled code names a primitive and an opcode by number, and both tables
/// only ever grow at the end — which until format 7 was a rule kept in a
/// comment. An image from a build with more of either would load in an older
/// one and call a number that is not there. Written into the header, the
/// older build can say so instead (#99).
fn machine() -> [u64; MACHINE_WORDS] {
    [
        crate::primitives::names().count() as u64,
        crate::code::OPCODE_COUNT as u64,
    ]
}

/// Write `heap` as an image.
///
/// **The heap is collected first**, and with no torture padding, for three
/// reasons that all point the same way. An image should not carry garbage, since every dead word is a word
/// downloaded in a browser. A copying collector leaves the live data
/// contiguous from the start of the heap, so the image is exactly the live
/// set with nothing to skip. And collecting makes the output *canonical* —
/// the same world saved twice gives the same bytes — which is what phase 2's
/// byte-for-byte fixpoint test will need.
///
/// Returns bytes rather than writing a file, because rule 2 says this crate
/// has no host dependencies. The host writes them to a file, to the browser's
/// storage, or to a download.
pub fn save(heap: &mut Heap) -> Vec<u8> {
    heap.collect_for_image();
    heap.advance_sequence();

    let scalars = heap.scalars().to_words();
    let machine = machine();
    let words = heap.words();

    // The checksum covers the scalars as well as the heap. A corrupted root
    // or program counter is as fatal as a corrupted cons cell and far harder
    // to see, since it is one word and looks like any other number.
    let body = || {
        scalars
            .iter()
            .chain(machine.iter())
            .chain(words.iter())
            .copied()
    };
    let header = [MAGIC, FORMAT_VERSION, words.len() as u64, checksum(body())];

    let mut bytes = Vec::with_capacity((HEADER_WORDS + words.len()) * 8);
    for word in header.into_iter().chain(body()) {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

/// Read an image back into a heap.
///
/// Little-endian on every target, stated rather than assumed: wasm32 and the
/// machines Quokka runs on natively are all little-endian, so this costs
/// nothing today and means an image moves between them. A big-endian host
/// would byte-swap here and nowhere else.
///
/// `soft_limit` is the loading host's, not the saving one's — see
/// the heap's `from_image`, which says why.
///
/// # Why it verifies
///
/// The checksum proves the bytes are the bytes that were written; it says
/// nothing about whether those bytes were a *sound* heap, which a collector
/// bug, or a person, could have made them not be. So after the checksum come
/// three checks, each on what the next one trusts: the scalars are numbers
/// that fit this heap; [`Heap::verify`] walks the heap and checks every
/// object's type, size and references, and the registers; and every compiled
/// function in it passes the bytecode verifier ([`crate::code::verify`]),
/// which is the promise `Vm::start` makes for whatever it is given.
///
/// Checking once at load rather than on every access is the same bargain the
/// bytecode verifier makes (chapter 25): pay a linear pass when the thing
/// arrives, and then trust it completely while it runs. The JVM loads a
/// class the same way, for the same reason. **No image, however made, may
/// panic the machine** — it is refused here, with a reason.
pub fn load(bytes: &[u8], soft_limit: usize) -> Result<Heap, ImageError> {
    if bytes.len() % 8 != 0 {
        return Err(ImageError::NotWholeWords { bytes: bytes.len() });
    }
    if bytes.len() < HEADER_WORDS * 8 {
        return Err(ImageError::TooShort { bytes: bytes.len() });
    }
    let word_at = |i: usize| {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&bytes[i * 8..i * 8 + 8]);
        u64::from_le_bytes(buf)
    };

    let magic = word_at(0);
    if magic != MAGIC {
        return Err(ImageError::NotAnImage { found: magic });
    }
    let version = word_at(1);
    if version != FORMAT_VERSION {
        return Err(ImageError::WrongVersion {
            found: version,
            expected: FORMAT_VERSION,
        });
    }

    // Compared as the file's own 64-bit number: cast to `usize` first, a
    // length past 2^32 would have wrapped on wasm32 and might have matched.
    let total = bytes.len() / 8;
    let found = total - HEADER_WORDS;
    let claimed = word_at(2);
    if claimed != found as u64 {
        return Err(ImageError::WrongLength { claimed, found });
    }

    let sum = checksum((4..total).map(word_at));
    let stored = word_at(3);
    if sum != stored {
        return Err(ImageError::Corrupt {
            claimed: stored,
            found: sum,
        });
    }

    let needs = [word_at(4 + Scalars::WORDS), word_at(5 + Scalars::WORDS)];
    let has = machine();
    if needs[0] > has[0] || needs[1] > has[1] {
        return Err(ImageError::NeedsNewerMachine {
            primitives: needs[0],
            opcodes: needs[1],
        });
    }

    let scalar_words: Vec<u64> = (0..Scalars::WORDS).map(|i| word_at(4 + i)).collect();
    let scalars = Scalars::from_words(&scalar_words);
    // Each of these becomes a `usize` index. One that fits the heap fits a
    // `usize` on any target, so checking it against the heap is checking the
    // cast as well.
    let fits = |n: u64| n <= found as u64;
    for (which, ok) in [
        ("static-area", fits(scalars.static_end)),
        ("program-counter", fits(scalars.pc)),
        ("stack-pointer", fits(scalars.sp)),
        ("frame-pointer", fits(scalars.fp)),
        ("mark-pointer", scalars.mp == u64::MAX || fits(scalars.mp)),
        ("mode", scalars.mode <= u8::MAX as u64),
    ] {
        if !ok {
            return Err(ImageError::Inconsistent(Inconsistency::BadRegister {
                which,
            }));
        }
    }

    let words: Vec<u64> = (HEADER_WORDS..total).map(word_at).collect();
    let mut heap = Heap::from_image(words, scalars, soft_limit);
    heap.verify().map_err(ImageError::Inconsistent)?;

    let functions = heap.objects_of(ObjectType::Function);
    heap.mutate(|mc| {
        for at in functions {
            let function = Value::object(at).expect("an index the heap just gave");
            crate::code::verify::one(mc, function)
                .map_err(|e| ImageError::BadCode(format!("{e:?}")))?;
        }
        Ok(())
    })?;
    Ok(heap)
}

#[cfg(test)]
#[path = "unit/image.rs"]
mod tests;
