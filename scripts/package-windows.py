from pathlib import Path
import hashlib, json, zipfile, shutil
root = Path(__file__).resolve().parents[1]
out = root / 'artifacts/windows'
release = root / 'target/x86_64-pc-windows-gnu/release'
version = json.loads((root / 'package.json').read_text())['version']
# Remove only the obsolete application produced by older versions of this packager.
(out / 'AirVault-offline.exe').unlink(missing_ok=True)
loader = release / 'WebView2Loader.dll'
if not loader.is_file():
    raise SystemExit('WebView2Loader.dll missing: refusing incomplete Windows package')
shutil.copy2(loader, out / 'WebView2Loader.dll')
documents = ['README.md', 'SECURITY.md', 'docs/testing.md', 'docs/implementation-status.md',
             'docs/validation.md', 'docs/application-identity.md', 'docs/multisig.md', 'docs/payload-inspector.md', 'docs/sparrow-interoperability.md', 'THIRD_PARTY_NOTICES.md']
for source in documents:
    (out / source).parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(root / source, out / source)
references = ['bitcoin-whitepaper.pdf', 'README.txt']
for name in references:
    destination = out / 'docs/reference' / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(root / 'public/reference' / name, destination)
metadata = json.loads((release / 'airvault-build-info.json').read_text())
if metadata['version'] != version:
    raise SystemExit('Build version does not match package version')
metadata['binarySha256'] = hashlib.sha256((out / 'AirVault.exe').read_bytes()).hexdigest()
(out / 'BUILD-INFO.json').write_text(json.dumps(metadata, indent=2) + '\n')
names = ['AirVault.exe', 'WebView2Loader.dll', 'BUILD-INFO.json', *documents,
         *['docs/reference/' + name for name in references]]
with zipfile.ZipFile(out / f'AirVault_{version}_windows-x64-portable.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
    for name in names:
        archive.write(out / name, 'AirVault/' + name)
files = sorted([*out.glob('*.exe'), *out.glob('*.dll'), *out.glob('*.zip'), out / 'BUILD-INFO.json'])
(out / 'SHA256SUMS.txt').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest() + '  ' + p.name + '\n' for p in files))
print('Complete Windows portable ZIP, build identity, offline reference and SHA256SUMS created.')
