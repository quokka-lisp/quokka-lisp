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

use crate::heap::{Heap, Scalars};

/// `"QUOKKA\0\0"` read as a little-endian `u64`.
///
/// A magic number so that a file which is not an image is refused as one
/// rather than loaded as a heap full of nonsense. Eight bytes, so it is also
/// the alignment the rest of the format assumes.
pub const MAGIC: u64 = u64::from_le_bytes(*b"QUOKKA\0\0");

/// The format this build writes, and the only one it reads.
///
/// There is no migration path and deliberately so: phase 2 rebuilds the image
/// from source with nothing but `cargo`, so an image that cannot be read is a
/// rebuild rather than a loss. That stops being true the moment a user's own
/// work lives in an image, which is when this number starts to matter and a
/// reader for older versions has to exist.
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
/// vector, and markers have a type of their own (#73).
pub const FORMAT_VERSION: u64 = 5;

/// Words before the heap's own: magic, version, length, checksum, scalars.
pub const HEADER_WORDS: usize = 4 + Scalars::WORDS;

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
    /// The header's word count does not match the bytes that follow.
    WrongLength {
        /// What the header claimed.
        claimed: usize,
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
fn checksum(words: &[u64]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for word in words {
        for byte in word.to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
    }
    hash
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

    let scalars = heap.scalars();
    let words = heap.words();

    // The checksum covers the scalars as well as the heap. A corrupted root
    // or program counter is as fatal as a corrupted cons cell and far harder
    // to see, since it is one word and looks like any other number.
    let mut header = Vec::with_capacity(HEADER_WORDS);
    header.push(MAGIC);
    header.push(FORMAT_VERSION);
    header.push(words.len() as u64);
    let scalar_words = scalars.to_words();
    let sum = {
        let mut both = Vec::with_capacity(Scalars::WORDS + words.len());
        both.extend_from_slice(&scalar_words);
        both.extend_from_slice(words);
        checksum(&both)
    };
    header.push(sum);
    header.extend_from_slice(&scalar_words);
    debug_assert_eq!(header.len(), HEADER_WORDS);

    let mut bytes = Vec::with_capacity((HEADER_WORDS + words.len()) * 8);
    for word in header.iter().chain(words.iter()) {
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
/// The last thing [`load`] does is run [`Heap::verify`], which walks the heap
/// linearly and checks that every reference lands on an object boundary. The
/// checksum proves the bytes are the bytes that were written; it says nothing
/// about whether those bytes were a *sound* heap, which a collector bug could
/// have made them not be.
///
/// Checking once at load rather than on every access is the same bargain the
/// bytecode verifier makes (`crate::code::verify`, and chapter 25): pay a
/// linear pass when the thing arrives, and then trust it completely while it
/// runs. The JVM loads a class the same way, for the same reason.
pub fn load(bytes: &[u8], soft_limit: usize) -> Result<Heap, ImageError> {
    if bytes.len() < HEADER_WORDS * 8 || bytes.len() % 8 != 0 {
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

    let claimed = word_at(2) as usize;
    let found = bytes.len() / 8 - HEADER_WORDS;
    if claimed != found {
        return Err(ImageError::WrongLength { claimed, found });
    }

    let mut body = Vec::with_capacity(Scalars::WORDS + found);
    for i in 0..Scalars::WORDS + found {
        body.push(word_at(4 + i));
    }
    let sum = checksum(&body);
    let stored = word_at(3);
    if sum != stored {
        return Err(ImageError::Corrupt {
            claimed: stored,
            found: sum,
        });
    }

    let scalars = Scalars::from_words(&body[..Scalars::WORDS]);
    let words = body[Scalars::WORDS..].to_vec();
    let heap = Heap::from_image(words, scalars, soft_limit);
    match heap.verify() {
        Ok(()) => Ok(heap),
        Err(e) => Err(ImageError::Inconsistent(e)),
    }
}

#[cfg(test)]
#[path = "unit/image.rs"]
mod tests;
