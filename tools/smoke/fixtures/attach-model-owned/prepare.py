"""Owned AttachModel asset-view preparation/admission. Never launches a process.
Inputs and generated binaries stay outside the repository. Native Data is read
only; new symlinks/files are created exclusively below --new-root. Symlinks are
not an OS write-protection mechanism: read-only use is a contract plus hash seals.
"""
import argparse,hashlib,json,os,uuid
from pathlib import Path
from make_nif import build_nif
MODEL=Path('meshes/nvf_smoke/attach_model.nif')
def require(v,message):
    if not v:raise ValueError(message)
def sha(path):
    digest=hashlib.sha256()
    with Path(path).open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024),b''):digest.update(chunk)
    return digest.hexdigest()
def read(path):return json.loads(Path(path).read_bytes())
def write_new(path,value):
    with Path(path).open('x',encoding='utf-8',newline='\n') as f:json.dump(value,f,indent=2);f.write('\n')
def inventory(path,digest):
    require(sha(path)==digest,'inventory pin changed');value=read(path)
    require(value.get('schema')=='nvf.attach-model.read-only-game-inputs/1','wrong inventory schema')
    root=Path(value['sourceData']).resolve(strict=True);require(root.is_dir(),'source Data missing')
    rows=value['files'];require(isinstance(rows,list) and rows,'explicit input inventory required')
    seen=set();kinds=set()
    for row in rows:
        name=row['name'];require(isinstance(name,str) and Path(name).name==name and '/' not in name and '\\' not in name and ':' not in name,'plain input filename required')
        require(name.casefold() not in seen,'duplicate input name');seen.add(name.casefold())
        kind=row['kind'];require(kind in ('master','bsa') and Path(name).suffix.casefold() in (('.esm','.esp') if kind=='master' else ('.bsa',)),'wrong input kind');kinds.add(kind)
        source=root/name;require(source.is_file(),'selected input absent');resolved=source.resolve(strict=True)
        require(str(resolved)==row['resolvedSource'],'resolved source authority changed')
        require(source.stat().st_size==row['bytes'] and sha(source)==row['SHA256'],'selected source hash/size changed')
    require(kinds=={'master','bsa'},'explicit master and BSA inventory required')
    return value,root


def require_disjoint_roots(root,source):
    # Resolve directory aliases before comparing path components, not string prefixes.
    # A view inside source Data would write to the read-only source; the reverse
    # containment also cannot describe an independently owned output root.
    root=Path(root).resolve();source=Path(source).resolve(strict=True)
    require(not (root.is_relative_to(source) or source.is_relative_to(root)),
            'owned root and source Data must be disjoint')

def create(new_root,inventory_path=None,inventory_sha=None):
    root=Path(new_root).resolve();require(not root.exists(),'new owned root already exists; never overwrite')
    require((inventory_path is None)==(inventory_sha is None),'inventory path and SHA must be supplied together')
    inputs,source=(inventory(inventory_path,inventory_sha) if inventory_path else (None,None))
    if source is not None:require_disjoint_roots(root,source)  # before the first mkdir
    root.mkdir();data=root/'Data';home=root/'home';model=data/MODEL;model.parent.mkdir(parents=True);home.mkdir()
    nonce=uuid.uuid4().hex;payload=build_nif()
    with model.open('xb') as f:f.write(payload)
    if inputs:
        for row in inputs['files']:
            dest=data/row['name'];require(not dest.exists(),'owned link collision')
            dest.symlink_to(Path(row['resolvedSource']))
            require(dest.resolve(strict=True)==Path(row['resolvedSource']) and sha(dest)==row['SHA256'],'link/hash admission failed')
        inventory(inventory_path,inventory_sha)  # source seals after owned link creation
    manifest={'schema':'nvf.attach-model.owned-view/1','root':str(root),'nonce':nonce,'data':str(data),'home':str(home),
              'modelRelative':MODEL.as_posix(),'modelSHA256':hashlib.sha256(payload).hexdigest(),'modelBytes':len(payload),
              'inventoryPath':str(Path(inventory_path).resolve()) if inputs else None,'inventorySHA256':inventory_sha if inputs else None,
              'sourceData':str(source) if source else None,'linkedInputs':inputs['files'] if inputs else [],
              'declaredGameInputsLinked':bool(inputs),'readOnlyByContractAndHashSealsNotAccessControl':True,
              'normalArtifactOrParserOrGameAcceptance':False}
    write_new(home/'ATTACH-SMOKE-HOME.json',{'nonce':nonce,'root':str(root)})
    write_new(root/'OWNED-ASSET-VIEW.json',manifest)
    return manifest

def admit(manifest_path,manifest_sha,data,home,for_game=False):
    manifest_path=Path(manifest_path).resolve(strict=True);require(sha(manifest_path)==manifest_sha,'owned manifest changed');m=read(manifest_path)
    require(m.get('schema')=='nvf.attach-model.owned-view/1','wrong owned manifest')
    require(m.get('modelRelative')==MODEL.as_posix(),'wrong explicit model VFS path')
    require(type(m.get('declaredGameInputsLinked')) is bool,'explicit boolean input-view declaration required')
    require(m.get('readOnlyByContractAndHashSealsNotAccessControl') is True,'read-only contract declaration missing')
    require(m.get('normalArtifactOrParserOrGameAcceptance') is False,'input manifest must not assert game acceptance')
    require(type(for_game) is bool,'game admission selector must be a boolean')
    root=Path(m['root']).resolve(strict=True);require(manifest_path==root/'OWNED-ASSET-VIEW.json','manifest not at owned root')
    data=Path(data).resolve(strict=True);home=Path(home).resolve(strict=True)
    require(data==root/'Data' and home==root/'home' and data!=home,'Data/HOME must be distinct exact owned children')
    require(m['data']==str(data) and m['home']==str(home),'manifest Data/HOME mismatch')
    marker=read(home/'ATTACH-SMOKE-HOME.json');require(marker=={'nonce':m['nonce'],'root':str(root)},'owned HOME marker mismatch')
    model=data/MODEL;require(model.resolve(strict=True)==model and sha(model)==m['modelSHA256'] and model.stat().st_size==m['modelBytes'],'authored model changed/escaped')
    require(m['modelSHA256']==hashlib.sha256(build_nif()).hexdigest(),'model not exact authored generator bytes')
    if m.get('sourceData') is not None:require_disjoint_roots(root,Path(m['sourceData']))
    if m['declaredGameInputsLinked']:
        inv,source=inventory(m['inventoryPath'],m['inventorySHA256']);require(inv['files']==m['linkedInputs'] and str(source)==m['sourceData'],'inventory changed')
        require_disjoint_roots(root,source)
        for row in m['linkedInputs']:
            p=data/row['name'];require(p.is_symlink() and str(p.resolve(strict=True))==row['resolvedSource'] and sha(p)==row['SHA256'],'owned master/BSA view changed')
    else:require(not for_game,'recorder-only view has no declared masters/BSAs; no game admission')
    allowed={MODEL.as_posix(),*[row['name'] for row in m['linkedInputs']]}
    actual={p.relative_to(data).as_posix() for p in data.rglob('*') if p.is_file()}
    require(actual==allowed,'undeclared asset input or missing declared input')
    return {'inputIntegrityAdmitted':True,'forGame':for_game,'gameOrDecoderOrArtifactAccepted':False,
            'data':str(data),'home':str(home),'manifest':str(manifest_path),'manifestSHA256':manifest_sha,
            'modelSHA256':m['modelSHA256'],'masterCount':sum(x['kind']=='master' for x in m['linkedInputs']),
            'bsaCount':sum(x['kind']=='bsa' for x in m['linkedInputs']),
            'env':{'NVF_PLAY_TEST_ATTACH_MODEL_HOME':str(home),'NVF_PLAY_TEST_ATTACH_MODEL_ASSET_VIEW':str(manifest_path),'NVF_PLAY_TEST_ATTACH_MODEL_PROBE_PATH':m['modelRelative']}}

def main():
    p=argparse.ArgumentParser();s=p.add_subparsers(dest='command',required=True)
    c=s.add_parser('create');c.add_argument('--new-root',type=Path,required=True);c.add_argument('--inventory',type=Path);c.add_argument('--inventory-sha')
    a=s.add_parser('check');a.add_argument('--manifest',type=Path,required=True);a.add_argument('--manifest-sha',required=True);a.add_argument('--data',type=Path,required=True);a.add_argument('--home',type=Path,required=True);a.add_argument('--for-game',action='store_true')
    v=p.parse_args();result=create(v.new_root,v.inventory,v.inventory_sha) if v.command=='create' else admit(v.manifest,v.manifest_sha,v.data,v.home,v.for_game)
    print(json.dumps(result))
if __name__=='__main__':main()
