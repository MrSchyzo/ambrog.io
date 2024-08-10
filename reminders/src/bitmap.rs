use std::num::NonZeroUsize;

struct BitmapIterator<'a> {
    underlying: &'a Bitmap,
    current_index: Option<usize>,
}

impl<'a> Iterator for BitmapIterator<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self
            .current_index
            .and_then(|idx| self.underlying.next_set(idx));
        self.current_index = result;
        result
    }
}

pub enum Bitmap {
    Byte(u8),
    Short(u16),
    Int(u32),
    Longs(Vec<u64>),
}

impl Bitmap {
    fn of_size(size: NonZeroUsize) -> Self {
        match size.get() {
            1..=8 => Self::Byte(0u8),
            9..=16 => Self::Short(0u16),
            17..=32 => Self::Int(0u32),
            x => Self::Longs(Vec::with_capacity(x / 64 + 1)),
        }
    }

    pub fn new_truncated(size: NonZeroUsize, bits: Vec<usize>) -> Self {
        let mut result = Self::of_size(size);
        for &bit in bits.iter() {
            result.set(bit)
        }
        if bits.is_empty() {
            result.set(0usize);
        }
        result
    }

    pub fn set(&mut self, position: usize) {
        match self {
            Bitmap::Byte(ref mut x) => *x |= 1u8 << position,
            Bitmap::Short(ref mut x) => *x |= 1u16 << position,
            Bitmap::Int(ref mut x) => *x |= 1u32 << position,
            Bitmap::Longs(ref mut v) if position < v.len() * 64 => {
                v[position / 64] |= 1u64 << (position % 64)
            }
            _ => (),
        }
    }

    pub fn get(&self, position: usize) -> bool {
        match self {
            Bitmap::Byte(x) => x & (1u8 << position) != 0,
            Bitmap::Short(x) => x & (1u16 << position) != 0,
            Bitmap::Int(x) => x & (1u32 << position) != 0,
            Bitmap::Longs(v) if position < v.len() * 64 => {
                v[position / 64] & (1u64 << (position % 64)) != 0
            }
            _ => false,
        }
    }

    fn len(&self) -> usize {
        match self {
            Bitmap::Byte(_) => 8,
            Bitmap::Short(_) => 16,
            Bitmap::Int(_) => 32,
            Bitmap::Longs(v) => v.len() * 64,
        }
    }

    pub fn next_set(&self, from: usize) -> Option<usize> {
        (from + 1..self.len()).find(|&i| self.get(i))
    }

    pub fn iter(&self, from: usize) -> impl Iterator<Item = usize> + '_ {
        BitmapIterator {
            underlying: self,
            current_index: Some(from),
        }
    }

    pub fn first_set(&self) -> Option<usize> {
        self.next_set(0usize)
    }
}

impl From<u8> for Bitmap {
    fn from(value: u8) -> Self {
        Self::Byte(value)
    }
}

impl From<u16> for Bitmap {
    fn from(value: u16) -> Self {
        Self::Short(value)
    }
}

impl From<u32> for Bitmap {
    fn from(value: u32) -> Self {
        Self::Int(value)
    }
}

impl From<u64> for Bitmap {
    fn from(value: u64) -> Self {
        Self::Longs(vec![value])
    }
}

impl From<Vec<u64>> for Bitmap {
    fn from(value: Vec<u64>) -> Self {
        Self::Longs(value)
    }
}

impl From<Vec<u8>> for Bitmap {
    fn from(value: Vec<u8>) -> Self {
        match value.len() {
            1 => Self::Byte(value[0]),
            2 => Self::Short(
                value
                    .into_iter()
                    .enumerate()
                    .fold(0u16, |v, (i, x)| v | ((x as u16) << (i * 8))),
            ),
            3..=4 => Self::Int(
                value
                    .into_iter()
                    .enumerate()
                    .fold(0u32, |v, (i, x)| v | ((x as u32) << (i * 8))),
            ),
            _ => Self::Longs(
                value
                    .chunks(8)
                    .map(|x| {
                        x.iter()
                            .enumerate()
                            .fold(0u64, |v, (i, x)| v | ((*x as u64) << (i * 8)))
                    })
                    .collect(),
            ),
        }
    }
}

impl From<Bitmap> for Vec<u8> {
    fn from(value: Bitmap) -> Self {
        match value {
            Bitmap::Byte(x) => vec![x],
            Bitmap::Short(x) => split_u16(x),
            Bitmap::Int(x) => split_u32(x),
            Bitmap::Longs(v) => v
                .into_iter()
                .flat_map(|x| split_u64(x).into_iter())
                .collect(),
        }
    }
}

fn split_u16(x: u16) -> Vec<u8> {
    (0u8..2u8)
        .map(|l| (((0xFF << (l * 8)) & x) >> (l * 8)) as u8)
        .collect::<Vec<u8>>()
}

fn split_u32(x: u32) -> Vec<u8> {
    (0u8..4u8)
        .map(|l| (((0xFF << (l * 8)) & x) >> (l * 8)) as u8)
        .collect::<Vec<u8>>()
}

fn split_u64(x: u64) -> Vec<u8> {
    (0u8..8u8)
        .map(|l| (((0xFF << (l * 8)) & x) >> (l * 8)) as u8)
        .collect::<Vec<u8>>()
}
