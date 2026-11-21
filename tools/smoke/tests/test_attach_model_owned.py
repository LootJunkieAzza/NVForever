"""Actual canonical AttachModel launch/owned input boundary; no game engine.
UNRUN source proposal. Requires the reviewed scenario to be registered first.
"""
import argparse,hashlib,json,os,subprocess,sys,tempfile,unittest
from pathlib import Path
from unittest import mock
REPO=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(REPO/'tools/smoke'))
sys.path.insert(0,str(REPO/'tools/smoke/fixtures/attach-model-owned'))
from smoke import runner,cli
from smoke.scenarios import load_text
from smoke.plan import plan
import prepare
HOME='NVF_PLAY_TEST_ATTACH_MODEL_HOME'
VIEW='NVF_PLAY_TEST_ATTACH_MODEL_ASSET_VIEW'
MODEL_PATH='NVF_PLAY_TEST_ATTACH_MODEL_PROBE_PATH'
NAME='nif-reference-attach-owned'
class AttachOwnedInputs(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix='nvf owned attach with spaces ');self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name)
        self.manifest=prepare.create(self.root/'owned view');self.mf=self.root/'owned view/OWNED-ASSET-VIEW.json';self.digest=prepare.sha(self.mf)
        self.data=Path(self.manifest['data']);self.home=Path(self.manifest['home']);self.out=self.root/'out';self.out.mkdir()
        self.suite=load_text((REPO/'tools/smoke/scenarios.toml').read_text(encoding='utf-8'), (REPO/'tools/smoke/scenarios.toml').parent)
        selected=[s for s in self.suite.scenarios if s.name==NAME];self.assertEqual(len(selected),1,'register exact reviewed table before this test runs')
        self.launch=plan(self.suite,selected)[0]
        child=self.root/'record.py';child.write_text("""import json,os,sys,hashlib
from pathlib import Path
a=sys.argv[1:];data=Path(a[a.index('--data')+1]);probe_path=os.environ['NVF_PLAY_TEST_ATTACH_MODEL_PROBE_PATH'];model=data/probe_path
home=Path(os.environ['NVFOREVER_HOME']);view=Path(os.environ['NVF_PLAY_TEST_ATTACH_MODEL_ASSET_VIEW'])
print('attach input receipt '+json.dumps({'argv':a,'cwd':str(Path.cwd()),'data':str(data),'home':str(home),'view':str(view),'view_sha':hashlib.sha256(view.read_bytes()).hexdigest(),'model_sha':hashlib.sha256(model.read_bytes()).hexdigest(),'model_bytes':model.stat().st_size,'probe_path':probe_path,'home_marker':json.loads((home/'ATTACH-SMOKE-HOME.json').read_bytes()),'console':os.environ.get('NVF_PLAY_TEST_CONSOLE'),'probe':os.environ.get('NVF_PLAY_TEST_ATTACH_MODEL_PROBE'),'clock':os.environ.get('NVF_PLAY_AIDS_FROM_SCRIPTS'),'logging':os.environ.get('RUST_LOG'),'foreign_aid':os.environ.get('NVF_PLAY_TEST_FOREIGN_ATTACH')}))
""",encoding='utf-8')
        self.exe=self.root/('record.cmd' if os.name=='nt' else 'record.sh')
        if os.name=='nt':self.exe.write_text(f'@"{sys.executable}" "{child}" %*\n',encoding='utf-8')
        else:self.exe.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{child}" "$@"\n',encoding='utf-8');self.exe.chmod(0o755)
        self.env=runner.Env(repo=REPO,out=self.out,exe=self.exe,data=self.data,fnvtool=None)
    def assert_not_started(self,action):
        with mock.patch.object(runner.subprocess,'Popen',side_effect=AssertionError('inadmissible fixture reached Popen')) as start:
            with self.assertRaises(ValueError):action()
            start.assert_not_called()
    def test_registered_launch_delivers_exact_owned_data_home_and_model_to_actual_child(self):
        admitted=prepare.admit(self.mf,self.digest,self.data,self.home,False)
        with mock.patch.dict(os.environ,{**admitted['env'],'NVF_PLAY_TEST_FOREIGN_ATTACH':'do not inherit','RUST_LOG':'off'}):
            self.assertEqual(runner.missing_requirements(self.launch.scenarios[0]),[])
            self.assertEqual(runner.launch_timeout(self.launch),180)
            code,err,_=runner.run_launch(self.launch,self.env,None)
        self.assertEqual((code,err),(0,''));rows=[l[len('attach input receipt '):] for l in (self.out/(NAME+'.log')).read_text(encoding='utf-8').splitlines() if l.startswith('attach input receipt ')];self.assertEqual(len(rows),1)
        r=json.loads(rows[0]);self.assertEqual(r['data'],str(self.data));self.assertEqual(r['home'],str(self.home));self.assertEqual(r['view'],str(self.mf));self.assertEqual(r['view_sha'],self.digest);self.assertEqual(r['model_sha'],self.manifest['modelSHA256']);self.assertEqual(r['model_bytes'],self.manifest['modelBytes']);self.assertEqual(r['probe_path'],self.manifest['modelRelative']);self.assertEqual(r['home_marker']['nonce'],self.manifest['nonce']);self.assertEqual(r['cwd'],str(self.out));self.assertEqual(r['probe'],'1');self.assertEqual(r['clock'],'1');self.assertEqual(r['logging'],'info,nvf_play::script=debug');self.assertIsNone(r['foreign_aid']);self.assertEqual(r['argv'],['--data',str(self.data),*self.launch.args,'--run-seconds','20']);self.assertEqual(r['console'],self.launch.env['NVF_PLAY_TEST_CONSOLE'])
    def test_wrong_data_refuses_before_launch(self):
        other=self.root/'other Data';other.mkdir();self.assert_not_started(lambda:prepare.admit(self.mf,self.digest,other,self.home,False))
    def test_wrong_home_refuses_before_launch(self):
        other=self.root/'other home';other.mkdir();self.assert_not_started(lambda:prepare.admit(self.mf,self.digest,self.data,other,False))
    def test_modified_model_refuses_before_launch(self):
        p=self.data/prepare.MODEL;p.write_bytes(p.read_bytes()+b'authored corruption');self.assert_not_started(lambda:prepare.admit(self.mf,self.digest,self.data,self.home,False))
    def test_recorder_only_view_refuses_game_input_admission(self):
        self.assert_not_started(lambda:prepare.admit(self.mf,self.digest,self.data,self.home,True))
    def test_existing_owned_root_is_never_overwritten(self):
        before={p.relative_to(self.root).as_posix():prepare.sha(p) for p in self.root.rglob('*') if p.is_file()}
        self.assert_not_started(lambda:prepare.create(self.root/'owned view'))
        after={p.relative_to(self.root).as_posix():prepare.sha(p) for p in self.root.rglob('*') if p.is_file()};self.assertEqual(before,after)
    def test_direct_run_launch_missing_or_empty_required_scalar_refuses(self):
        for missing in [HOME,VIEW,MODEL_PATH]:
            for value in [None,'']:
                with self.subTest(missing=missing,value=value),mock.patch.dict(os.environ,{HOME:str(self.home),VIEW:str(self.mf),MODEL_PATH:self.manifest['modelRelative']}):
                    if value is None:os.environ.pop(missing,None)
                    else:os.environ[missing]=value
                    with mock.patch.object(runner.subprocess,'Popen',side_effect=AssertionError('missing scalar reached Popen')) as start:
                        code,error,_=runner.run_launch(self.launch,self.env,None)
                    start.assert_not_called();self.assertIsNone(code);self.assertIn(missing,error)
    def test_actual_cli_requires_skip_without_child(self):
        for missing in [HOME,VIEW,MODEL_PATH]:
            with self.subTest(missing=missing),mock.patch.dict(os.environ,{HOME:str(self.home),VIEW:str(self.mf),MODEL_PATH:self.manifest['modelRelative']}):
                os.environ.pop(missing,None)
                with mock.patch.object(runner.subprocess,'Popen',side_effect=AssertionError('missing requires reached Popen')) as start:
                    verdicts=cli.run_launches([self.launch],self.env,None,1,argparse.Namespace(context=3),lambda _:None)
                start.assert_not_called();self.assertEqual([v.status for v in verdicts],['SKIP']);self.assertIn(missing,verdicts[0].reason)
    def test_wrong_manifest_model_path_refuses_before_launch(self):
        m=json.loads(self.mf.read_bytes());m['modelRelative']='nvf_smoke/attach_model.nif'
        self.mf.write_text(json.dumps(m),encoding='utf-8');new_digest=prepare.sha(self.mf)
        self.assert_not_started(lambda:prepare.admit(self.mf,new_digest,self.data,self.home,False))
class AttachOwnedContainment(unittest.TestCase):
    """Synthetic integrity inputs only; these literal bytes are not ESM/BSA files."""
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix='nvf attach containment ')
        self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name)
        self.source=self.root/'source Data';self.source.mkdir()
        (self.source/'master.esm').write_bytes(b'authored integrity master, not game data')
        (self.source/'archive.bsa').write_bytes(b'authored integrity archive, not game data')
        self.inventory=self.root/'inventory.json'
        self.rows=[{'name':name,'kind':kind,'resolvedSource':str((self.source/name).resolve()),
                    'bytes':(self.source/name).stat().st_size,'SHA256':prepare.sha(self.source/name)}
                   for name,kind in [('master.esm','master'),('archive.bsa','bsa')]]
        self.inventory.write_text(json.dumps({'schema':'nvf.attach-model.read-only-game-inputs/1',
            'sourceData':str(self.source.resolve()),'files':self.rows}),encoding='utf-8')
        self.pin=prepare.sha(self.inventory)
    def source_tree(self):
        return {p.relative_to(self.source).as_posix():
                ('link',os.readlink(p)) if p.is_symlink() else ('dir',) if p.is_dir() else ('file',prepare.sha(p))
                for p in self.source.rglob('*')}
    def create_linked(self,root):
        try:return prepare.create(root,self.inventory,self.pin)
        except OSError as error:
            if os.name=='nt' and getattr(error,'winerror',None)==1314:
                self.skipTest('SETUP_UNSUPPORTED: Windows directory/file symlink privilege unavailable')
            raise
    def test_nested_new_root_refuses_before_mkdir_without_source_tree_change(self):
        nested=self.source/'must not be created';before=self.source_tree()
        with self.assertRaisesRegex(ValueError,'must be disjoint'):
            prepare.create(nested,self.inventory,self.pin)
        self.assertFalse(nested.exists());self.assertEqual(self.source_tree(),before)
    def test_resolved_directory_alias_to_nested_root_also_refuses_before_mkdir(self):
        alias=self.root/'source alias'
        try:alias.symlink_to(self.source,target_is_directory=True)
        except OSError as error:
            if os.name=='nt' and getattr(error,'winerror',None)==1314:
                self.skipTest('SETUP_UNSUPPORTED: Windows directory symlink privilege unavailable')
            raise
        before=self.source_tree();nested=alias/'must not be created'
        with self.assertRaisesRegex(ValueError,'must be disjoint'):
            prepare.create(nested,self.inventory,self.pin)
        self.assertFalse(nested.exists());self.assertEqual(self.source_tree(),before)
    def test_disjoint_string_prefix_sibling_still_admits_selected_inputs(self):
        sibling=self.root/'source Data owned';before=self.source_tree()
        manifest=self.create_linked(sibling);path=sibling/'OWNED-ASSET-VIEW.json'
        admitted=prepare.admit(path,prepare.sha(path),manifest['data'],manifest['home'],True)
        self.assertTrue(admitted['inputIntegrityAdmitted']);self.assertTrue(admitted['forGame'])
        self.assertFalse(admitted['gameOrDecoderOrArtifactAccepted'])
        self.assertEqual((admitted['masterCount'],admitted['bsaCount']),(1,1))
        self.assertEqual(self.source_tree(),before)
    def test_preexisting_unsafe_nested_view_is_refused_independently_by_admit(self):
        # Rehouse an otherwise valid synthetic view to represent an old unsafe
        # manifest. The admission guard must work without calling new create.
        sibling=self.root/'disjoint owned';manifest=self.create_linked(sibling)
        nested=self.source/'old unsafe owned';sibling.rename(nested)
        manifest.update(root=str(nested.resolve()),data=str((nested/'Data').resolve()),home=str((nested/'home').resolve()))
        path=nested/'OWNED-ASSET-VIEW.json';path.write_text(json.dumps(manifest),encoding='utf-8')
        (nested/'home/ATTACH-SMOKE-HOME.json').write_text(json.dumps({'nonce':manifest['nonce'],'root':manifest['root']}),encoding='utf-8')
        before=self.source_tree()
        with self.assertRaisesRegex(ValueError,'must be disjoint'):
            prepare.admit(path,prepare.sha(path),manifest['data'],manifest['home'],True)
        self.assertEqual(self.source_tree(),before)
    def test_changed_selected_inventory_input_still_refuses_before_new_root(self):
        new=self.root/'disjoint owned';(self.source/'master.esm').write_bytes(b'changed authored integrity bytes')
        before=self.source_tree()
        with self.assertRaisesRegex(ValueError,'selected source hash/size changed'):
            prepare.create(new,self.inventory,self.pin)
        self.assertFalse(new.exists());self.assertEqual(self.source_tree(),before)


if __name__=='__main__':unittest.main()
