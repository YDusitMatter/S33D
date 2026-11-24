"""Download one immutable, checksum-pinned source asset; never forward credentials."""
from pathlib import Path
import hashlib,json,os,urllib.request,urllib.error,urllib.parse

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*args,**kwargs):return None

root=Path(os.environ['AUDIT_ROOT']);root.mkdir(parents=True,exist_ok=False)
token=os.environ['GH_TOKEN'];repo='YDusitMatter/S33D';asset=int(os.environ['SOURCE_ASSET_ID'])
headers={'Authorization':'Bearer '+token,'Accept':'application/vnd.github+json','X-GitHub-Api-Version':'2022-11-28','User-Agent':'S33D-corresponding-source-audit'}
with urllib.request.urlopen(urllib.request.Request('https://api.github.com/repos/'+repo,headers=headers),timeout=60) as response:
    assert json.load(response)['private'],'Audit is authorized for the private preparation only'
headers['Accept']='application/octet-stream'
req=urllib.request.Request('https://api.github.com/repos/'+repo+'/releases/assets/'+str(asset),headers=headers)
try:response=urllib.request.build_opener(NoRedirect).open(req,timeout=90)
except urllib.error.HTTPError as error:
    if error.code not in (301,302,303,307):raise RuntimeError('Source download HTTP '+str(error.code)) from None
    location=error.headers['Location'];parts=urllib.parse.urlsplit(location)
    assert parts.scheme=='https' and parts.hostname.endswith('.githubusercontent.com')
    response=urllib.request.urlopen(urllib.request.Request(location,headers={'User-Agent':'S33D-source-audit'}),timeout=180)
path=root/'corresponding-source.zip'
with response,path.open('wb') as out:
    while chunk:=response.read(1024*1024):out.write(chunk)
with path.open('rb') as stream:digest=hashlib.file_digest(stream,'sha256').hexdigest()
assert digest==os.environ['EXPECTED_SOURCE_SHA256']
(root/'source-download.json').write_text(json.dumps({'source_asset_id':asset,'sha256':digest,'verified':True}),encoding='utf-8')
print('Downloaded corresponding source ZIP; SHA-256 verified.')
