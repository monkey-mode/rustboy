/// Manual binary serialization helpers for save states.
///
/// All multi-byte values are written/read in little-endian order.
/// Save order must match load order exactly.

// ---------------------------------------------------------------------------
// Write helpers
// ---------------------------------------------------------------------------

pub fn write_u8(buf: &mut Vec<u8>, v: u8) {
    buf.push(v);
}

pub fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

pub fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

pub fn write_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

pub fn write_bool(buf: &mut Vec<u8>, v: bool) {
    buf.push(if v { 1 } else { 0 });
}

/// Write length (u32 LE) then bytes – for variable-size slices.
pub fn write_bytes(buf: &mut Vec<u8>, data: &[u8]) {
    write_u32(buf, data.len() as u32);
    buf.extend_from_slice(data);
}

/// Write bytes directly without a length prefix – for fixed-size slices.
pub fn write_slice(buf: &mut Vec<u8>, data: &[u8]) {
    buf.extend_from_slice(data);
}

// ---------------------------------------------------------------------------
// Read helpers
// ---------------------------------------------------------------------------

pub fn read_u8(data: &[u8], off: &mut usize) -> u8 {
    let v = data[*off];
    *off += 1;
    v
}

pub fn read_u16(data: &[u8], off: &mut usize) -> u16 {
    let v = u16::from_le_bytes([data[*off], data[*off + 1]]);
    *off += 2;
    v
}

pub fn read_u32(data: &[u8], off: &mut usize) -> u32 {
    let v = u32::from_le_bytes([data[*off], data[*off + 1], data[*off + 2], data[*off + 3]]);
    *off += 4;
    v
}

pub fn read_u64(data: &[u8], off: &mut usize) -> u64 {
    let v = u64::from_le_bytes([
        data[*off],     data[*off + 1], data[*off + 2], data[*off + 3],
        data[*off + 4], data[*off + 5], data[*off + 6], data[*off + 7],
    ]);
    *off += 8;
    v
}

pub fn read_bool(data: &[u8], off: &mut usize) -> bool {
    let v = data[*off] != 0;
    *off += 1;
    v
}

/// Read a length-prefixed byte vector.
pub fn read_bytes(data: &[u8], off: &mut usize) -> Vec<u8> {
    let len = read_u32(data, off) as usize;
    let v = data[*off..*off + len].to_vec();
    *off += len;
    v
}

/// Read a fixed-size slice (no length prefix).
pub fn read_slice<'a>(data: &'a [u8], off: &mut usize, len: usize) -> &'a [u8] {
    let v = &data[*off..*off + len];
    *off += len;
    v
}
