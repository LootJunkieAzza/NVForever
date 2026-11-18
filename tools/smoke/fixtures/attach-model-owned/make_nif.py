"""Write only newly authored geometry to a new owned view; never installed Data.
No game asset is read. This is a source proposal, not a parser/render qualification.
"""
import struct
U32=lambda n:struct.pack('<I',n)
I32=lambda n:struct.pack('<i',n)
U16=lambda n:struct.pack('<H',n)
U8=lambda n:bytes([n])
F=lambda *v:struct.pack('<'+'f'*len(v),*v)
TEXT=lambda s:U32(len(s.encode()))+s.encode()
EXPORT=lambda s:U8(len(s.encode())+1)+s.encode()+b'\0'
IDENTITY=(1.,0.,0.,0.,1.,0.,0.,0.,1.)
def av(name,position,properties):
 return U32(name)+U32(0)+I32(-1)+U32(14)+F(*position)+F(*IDENTITY)+F(1.)+U32(len(properties))+b''.join(I32(p) for p in properties)+I32(-1)
def node(name,position,children):
 return av(name,position,[])+U32(len(children))+b''.join(I32(c) for c in children)+U32(0)
def shape(name,position,data,material,shader):
 return av(name,position,[material,shader])+I32(data)+I32(-1)+U32(0)+I32(-1)+U8(0)
def geometry(width):
 # Four authored vertices, two strip triangles, four UVs. The layout follows
 # the public nif_attachments/tests.rs builder, never copied game geometry.
 return (I32(0)+U16(4)+U8(0)+U8(0)+U8(1)+F(0.,0.,0.,width,0.,0.,0.,32.,0.,width,32.,0.)
         +U16(1)+U8(0)+F(0.,0.,0.,100.)+U8(0)+F(0.,0.,0.,1.,1.,0.,1.,1.)
         +U16(0)+I32(-1)+U16(2)+U16(1)+U16(4)+U8(1)+U16(0)+U16(1)+U16(2)+U16(3))
def material(gloss):
 return U32(0xffffffff)+U32(0)+I32(-1)+F(1.,0.,0.,0.,0.,0.,gloss,1.,1.)
def lit_shader():
 # Field sequence from the public engine/nvf-assets/src/nif_spawn.rs
 # authored shader builder. This prevents helper-shape classification;
 # production filtering is unchanged and GAME qualification is separate.
 return U32(0xffffffff)+U32(0)+I32(-1)+U16(1)+U32(1)+U32(0)+U32(0)+F(1.)+U32(3)+I32(-1)+F(0.)+U32(0)+F(4.,1.)

def build_nif():
 blocks=[('BSFadeNode',node(0,[32.,0.,80.],[1,2])),
         ('NiTriStrips',shape(1,[0.,0.,0.],3,5,7)),
         ('NiTriStrips',shape(1,[0.,40.,0.],4,6,7)),
         ('NiTriStripsData',geometry(32.)),('NiTriStripsData',geometry(64.)),
         ('NiMaterialProperty',material(11.)),('NiMaterialProperty',material(22.)),
         ('BSShaderPPLightingProperty',lit_shader())]
 strings=['OwnedAttachRoot','OwnedAttachShape'];types=list(dict.fromkeys(n for n,_ in blocks))
 out=(b'Gamebryo File Format, Version 20.2.0.7\n'+U32(0x14020007)+U8(1)+U32(11)+U32(len(blocks))+U32(34)
      +EXPORT('owned')+EXPORT('fixture')+EXPORT('synthetic')+U16(len(types)))
 out+=b''.join(TEXT(n) for n in types)+b''.join(U16(types.index(n)) for n,_ in blocks)+b''.join(U32(len(b)) for _,b in blocks)
 out+=U32(len(strings))+U32(max(map(len,strings)))+b''.join(TEXT(s) for s in strings)+U32(0)
 out+=b''.join(b for _,b in blocks)+U32(1)+I32(0)
 return out

# prepare.py is the single preparation/admission entry point. Keep this module
# limited to the authored byte generator so it cannot create a second manifest.
