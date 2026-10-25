/** Bounded JSON span reader. Input is validated first; selected scalar tokens retain original precision. */
export interface JsonText {readonly raw:string;readonly kind:'string'|'number'|'boolean'|'null'|'object'|'array';readonly members:ReadonlyMap<string,JsonText>;readonly items:readonly JsonText[]}
export function jsonText(text:string):JsonText {
 let index=0;let nodes=0;
 const whitespace=()=>{while(/\s/u.test(text[index]??'')&&index<text.length)index++;};
 const stringEnd=()=>{index++;while(index<text.length){const char=text[index++];if(char==='\\')index++;else if(char==='"')return;}throw Error('Invalid JSON string');};
 const read=(depth:number):JsonText=>{
  if(++nodes>20000||depth>20)throw Error('JSON structure exceeds bound');whitespace();const start=index;const char=text[index];
  const members=new Map<string,JsonText>();const items:JsonText[]=[];let kind:JsonText['kind'];
  if(char==='{'){
   kind='object';index++;whitespace();
   while(text[index]!=='}'){
    if(text[index]!=='"')throw Error('Invalid JSON member');const keyStart=index;stringEnd();const key:unknown=JSON.parse(text.slice(keyStart,index));
    if(typeof key!=='string'||members.has(key))throw Error('Duplicate JSON member');whitespace();if(text[index++]!==':')throw Error('Invalid member separator');
    members.set(key,read(depth+1));whitespace();if(text[index]!==',')break;index++;whitespace();
   }if(text[index++]!=='}')throw Error('Invalid object');
  }else if(char==='['){
   kind='array';index++;whitespace();while(text[index]!==']'){items.push(read(depth+1));whitespace();if(text[index]!==',')break;index++;}if(text[index++]!==']')throw Error('Invalid array');
  }else if(char==='"'){kind='string';stringEnd();}
  else if(char==='t'||char==='f'){kind='boolean';index+=char==='t'?4:5;}
  else if(char==='n'){kind='null';index+=4;}
  else {kind='number';while(/[0-9eE+.\-]/u.test(text[index]??'')&&index<text.length)index++;if(index===start)throw Error('Invalid number');}
  return {raw:text.slice(start,index),kind,members,items};
 };
 const result=read(0);whitespace();if(index!==text.length)throw Error('Trailing JSON');return result;
}
