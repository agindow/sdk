/** Frozen release content is separate from current editorial/API validation. */
import { docsTree, docsChannels, sdkMatrix, docPath, docPaths } from '../docs.js';
import type { DocPage, DocsTree } from './model.js';
import { releaseSnapshotInputs } from './releases.generated.js';
export interface DocsChannel { readonly id:string; readonly label:string; readonly display:string; readonly href:string; readonly npm:string; readonly pypi:string }
export interface ReleaseSnapshot { readonly format:'agindow.docs-release/1';readonly npm:string;readonly pypi:string;readonly source:string;readonly digest:string;readonly tree:DocsTree }
function releaseSnapshot(value:unknown):ReleaseSnapshot {
 if(typeof value!=='object'||value===null||!('format' in value)||value.format!=='agindow.docs-release/1'
   ||!('npm' in value)||typeof value.npm!=='string'||!('pypi' in value)||typeof value.pypi!=='string'
   ||!('source' in value)||typeof value.source!=='string'||!('digest' in value)||typeof value.digest!=='string'
   ||!('tree' in value)||typeof value.tree!=='object'||value.tree===null||!('pages' in value.tree)||!Array.isArray(value.tree.pages)) throw Error('Invalid release snapshot');
 return value as ReleaseSnapshot;
}
export const releaseSnapshots:readonly ReleaseSnapshot[]=releaseSnapshotInputs.map(releaseSnapshot);
export const archivedReleases=releaseSnapshots.filter(release=>release.npm!==sdkMatrix.node.version||release.pypi!==sdkMatrix.python.version);
export const releaseRoot=(version:string):string=>`/docs/versions/${version}`;
export function scopedDocPath(root:string,path:string):string {
 return /^\/docs(?:\/|$|#)/u.test(path)?root+path.slice(5):path;
}
function projectedTree(release:ReleaseSnapshot):DocsTree {
 const root=releaseRoot(release.npm);
 return {...release.tree,pages:release.tree.pages.map(page=>({...page,path:scopedDocPath(root,docPath(page)),
  sections:page.sections.map(section=>({...section,...(section.links?{links:section.links.map(link=>({...link,href:scopedDocPath(root,link.href)}))}:{})}))}))};
}
export interface DocsEdition { readonly tree:DocsTree;readonly channel:DocsChannel;readonly archived:boolean;readonly root:string;readonly path:string }
export function docsEdition(path:string):DocsEdition {
 const release=archivedReleases.find(item=>path===releaseRoot(item.npm)||path.startsWith(releaseRoot(item.npm)+'/'));
 if(release){
  const root=releaseRoot(release.npm);const sourcePath='/docs'+path.slice(root.length);
  const page=release.tree.pages.find(item=>docPaths(item).includes(sourcePath));
  return {tree:projectedTree(release),channel:{id:release.npm,label:'Older version',display:release.npm,href:root,npm:release.npm,pypi:release.pypi},archived:true,root,
   path:page?scopedDocPath(root,docPath(page)):path};
 }
 const channel:DocsChannel=docsChannels[0]??{id:'latest',label:'Latest',display:sdkMatrix.node.version,href:'/docs',npm:sdkMatrix.node.version,pypi:sdkMatrix.python.version};
 const page=docsTree.pages.find(item=>docPaths(item).includes(path));
 return {tree:docsTree,channel,archived:false,root:'/docs',path:page?docPath(page):path};
}
export const archivedDocRoutes=archivedReleases.flatMap(release=>release.tree.pages.filter(page=>page.published).flatMap(page=>docPaths(page).map(path=>({
 path:scopedDocPath(releaseRoot(release.npm),path),title:`${page.title} (${release.npm})`,asset:`/pages${scopedDocPath(releaseRoot(release.npm),path)}.html`,
}))));
export function versionTarget(channel:DocsChannel,currentPath:string):string {
 const current=docsEdition(currentPath);const page=current.tree.pages.find(page=>docPath(page)===current.path);
 if(!page)return channel.href;
 const targetTree=channel.id==='latest'?docsTree:archivedReleases.find(release=>release.npm===channel.npm)?.tree;
 const match=targetTree?.pages.find(item=>item.published&&item.family===page.family&&item.slug===page.slug);
 return match?scopedDocPath(channel.href,docPath(match)):channel.href;
}
export function archiveCanonicalPath(path:string):string {
 return docsEdition(path).path;
}
