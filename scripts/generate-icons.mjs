import sharp from 'sharp';
import { readFileSync, writeFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..');

const svgPath = join(root, 'assets/images/nir_logo.svg');
const winDir = join(root, 'crates/zed/resources/windows');
const pngDir = join(root, 'crates/zed/resources');

const sizes = [16, 32, 48, 64, 128, 256];

/**
 * Generate ICO file with all sizes as PNG data.
 * Modern Windows (10/11) fully supports PNG-compressed entries at all sizes.
 * ColorPlanes MUST be 0 for PNG entries (per MSDN spec).
 */
function buildIco(pngBuffers) {
  const numImages = sizes.length;
  const headerSize = 6;
  const entrySize = 16;

  // Calculate offsets
  let offset = headerSize + entrySize * numImages;
  const entries = sizes.map((size, i) => {
    const entry = {
      size,
      data: pngBuffers[i],
      offset,
      dataSize: pngBuffers[i].length,
    };
    offset += entry.dataSize;
    return entry;
  });

  const buf = Buffer.alloc(offset);

  // ICO header
  buf.writeUInt16LE(0, 0);           // reserved
  buf.writeUInt16LE(1, 2);           // type: ICO
  buf.writeUInt16LE(numImages, 4);   // image count

  // Directory entries
  let pos = headerSize;
  for (const e of entries) {
    const w = e.size === 256 ? 0 : e.size;
    buf.writeUInt8(w, pos);           // width (0 = 256)
    buf.writeUInt8(w, pos + 1);       // height
    buf.writeUInt8(0, pos + 2);       // color palette
    buf.writeUInt8(0, pos + 3);       // reserved
    buf.writeUInt16LE(0, pos + 4);    // color planes (0 for PNG data)
    buf.writeUInt16LE(32, pos + 6);   // bits per pixel
    buf.writeUInt32LE(e.dataSize, pos + 8);   // data size
    buf.writeUInt32LE(e.offset, pos + 12);    // data offset
    pos += entrySize;
  }

  // Image data (PNG)
  for (const e of entries) {
    e.data.copy(buf, e.offset);
  }

  return buf;
}

async function generateChannel(channelSuffix = '') {
  const svg = readFileSync(svgPath);

  // Generate PNGs at all sizes
  const pngs = await Promise.all(
    sizes.map(size => sharp(svg).resize(size, size).png().toBuffer())
  );

  // Build ICO
  const icoData = buildIco(pngs);
  const icoName = `app-icon${channelSuffix}.ico`;
  writeFileSync(join(winDir, icoName), icoData);
  console.log(`  ✓ ${icoName}`);

  // Standalone PNGs for Linux/macOS
  const pngName = `app-icon${channelSuffix}.png`;
  await sharp(svg).resize(256, 256).png().toFile(join(pngDir, pngName));
  console.log(`  ✓ ${pngName}`);

  const png2xName = `app-icon${channelSuffix}@2x.png`;
  await sharp(svg).resize(512, 512).png().toFile(join(pngDir, png2xName));
  console.log(`  ✓ ${png2xName}`);
}

async function main() {
  const channels = ['', '-dev', '-nightly', '-preview'];
  const channelNames = ['stable', 'dev', 'nightly', 'preview'];

  for (let i = 0; i < channels.length; i++) {
    console.log(`Generating ${channelNames[i]} icons...`);
    await generateChannel(channels[i]);
  }

  // Auto-updater icon (copy stable)
  const stableIco = readFileSync(join(winDir, 'app-icon.ico'));
  writeFileSync(join(root, 'crates/auto_update_helper/app-icon.ico'), stableIco);
  console.log('  ✓ auto-update helper icon');

  console.log('\n✅ All icons regenerated!');
}

main().catch(err => { console.error(err); process.exit(1); });
