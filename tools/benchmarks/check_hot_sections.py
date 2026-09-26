"""Check the dedicated function sections in an AVL BASIC Windows x64 PE.

Standard library only; reads the executable without running it. This verifies
the final image layout, not the identity of the compiler or linker that made it.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def inspect(path: Path) -> dict:
    data = path.read_bytes()

    def unpack(fmt: str, offset: int) -> tuple:
        size = struct.calcsize(fmt)
        require(0 <= offset <= len(data) - size, f"Truncated PE at file offset {offset:#x}")
        return struct.unpack_from(fmt, data, offset)

    require(data[:2] == b"MZ", "Missing DOS signature")
    pe = unpack("<I", 0x3C)[0]
    require(data[pe:pe + 4] == b"PE\0\0", "Missing PE signature")
    machine, section_count, _, _, _, optional_size, _ = unpack("<HHIIIHH", pe + 4)
    require(machine == 0x8664, f"Expected x64 machine 0x8664, got {machine:#x}")
    optional = pe + 24
    require(optional_size >= 144, "Optional header does not contain the exception directory")
    require(unpack("<H", optional)[0] == 0x20B, "Expected PE32+ optional header")
    image_base = unpack("<Q", optional + 24)[0]
    section_alignment = unpack("<I", optional + 32)[0]
    image_size = unpack("<I", optional + 56)[0]
    directory_count = unpack("<I", optional + 108)[0]
    require(section_alignment >= 4096 and section_alignment & (section_alignment - 1) == 0,
            "SectionAlignment must be a power of two of at least 4096 bytes")
    require(directory_count >= 4, "Missing exception directory")
    exception_rva, exception_size = unpack("<II", optional + 112 + 3 * 8)

    sections = []
    for index in range(section_count):
        header = optional + optional_size + index * 40
        name, virtual_size, rva, raw_size, raw_offset, _, _, _, _, flags = unpack(
            "<8sIIIIIIHHI", header)
        sections.append({
            "name": name.rstrip(b"\0").decode("ascii", errors="replace"),
            "rva": rva, "virtual_size": virtual_size,
            "raw_size": raw_size, "raw_offset": raw_offset, "flags": flags,
        })

    def unique_section(name: str) -> dict:
        matches = [section for section in sections if section["name"] == name]
        require(len(matches) == 1, f"Expected exactly one {name} section, found {len(matches)}")
        return matches[0]

    def offset_for_rva(rva: int, size: int) -> int:
        require(0 < rva < image_size and 0 < size <= image_size - rva,
                f"RVA range {rva:#x}+{size:#x} is outside the image")
        matches = [section for section in sections
                   if section["rva"] <= rva
                   and rva + size <= section["rva"] + min(section["virtual_size"], section["raw_size"])]
        require(len(matches) == 1, f"RVA range {rva:#x}+{size:#x} is not uniquely file-backed")
        offset = matches[0]["raw_offset"] + rva - matches[0]["rva"]
        require(offset + size <= len(data), f"Truncated data for RVA {rva:#x}")
        return offset

    pdata = unique_section(".pdata")
    require(exception_size > 0 and exception_size % 12 == 0,
            "Exception directory must contain complete RUNTIME_FUNCTION records")
    require(pdata["rva"] <= exception_rva
            and exception_rva + exception_size <= pdata["rva"] + pdata["virtual_size"],
            "Exception directory is not contained in .pdata")
    exception_offset = offset_for_rva(exception_rva, exception_size)
    functions = [unpack("<III", offset) for offset in
                 range(exception_offset, exception_offset + exception_size, 12)]

    checked = []
    for name in (".avlrun", ".avleval"):
        section = unique_section(name)
        rva, virtual_size, flags = section["rva"], section["virtual_size"], section["flags"]
        require(virtual_size > 0 and section["raw_size"] > 0, f"{name} is empty")
        require(rva % section_alignment == 0, f"{name} is not aligned to SectionAlignment")
        require(flags & 0x60000020 == 0x60000020, f"{name} lacks CODE, READ or EXECUTE")
        require(flags & 0x86000000 == 0, f"{name} is writable, discardable or not cached")
        require(rva + virtual_size <= image_size, f"{name} extends outside the image")
        ranges = [entry for entry in functions if entry[0] < rva + virtual_size and entry[1] > rva]
        require(len(ranges) == 1, f"{name} overlaps {len(ranges)} RUNTIME_FUNCTION records, expected one")
        begin, end, unwind_rva = ranges[0]
        require(begin == rva and begin < end <= rva + virtual_size,
                f"{name} function must start at the section RVA and end inside its virtual size")
        offset_for_rva(begin, end - begin)
        require(unwind_rva % 4 == 0, f"{name} unwind metadata is not DWORD aligned")
        unwind_offset = offset_for_rva(unwind_rva, 4)
        version_flags, _, code_count, _ = unpack("<BBBB", unwind_offset)
        version, unwind_flags = version_flags & 7, version_flags >> 3
        require(version in (1, 2), f"{name} has unsupported UNWIND_INFO version {version}")
        require(unwind_flags & ~7 == 0 and not (unwind_flags & 4 and unwind_flags & 3),
                f"{name} has invalid UNWIND_INFO flags {unwind_flags:#x}")
        unwind_size = 4 + ((code_count + 1) & ~1) * 2
        unwind_size += 12 if unwind_flags & 4 else 4 if unwind_flags & 3 else 0
        offset_for_rva(unwind_rva, unwind_size)
        checked.append({
            "name": name, "rva": f"0x{rva:x}", "preferred_address": f"0x{image_base + rva:x}",
            "virtual_size": virtual_size, "raw_size": section["raw_size"],
            "flags": f"0x{flags:08x}", "function_size": end - begin,
            "end_rva": f"0x{end:x}", "unwind_rva": f"0x{unwind_rva:x}",
            "unwind_version": version,
        })
    return {"ok": True, "exe": str(path.resolve()), "sha256": hashlib.sha256(data).hexdigest(),
            "format": "PE32+ x64", "section_alignment": section_alignment, "sections": checked}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("exe", type=Path, help="Final Windows x64 executable to inspect")
    args = parser.parse_args()
    try:
        result = inspect(args.exe)
    except (OSError, ValueError) as error:
        print(json.dumps({"ok": False, "exe": str(args.exe), "error": str(error)}), file=sys.stderr)
        return 1
    print(json.dumps(result, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
