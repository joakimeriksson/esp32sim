import {runBattery} from './battery.mjs';
onmessage=async ({data})=>{
  try {
    const response = await fetch('/provenance.json');
    if (!response.ok) throw Error(`provenance: HTTP ${response.status}`);
    const provenance = await response.json();
    await runBattery(async name=>{
      const asset = await fetch('/asset/'+name);
      if (!asset.ok) throw Error(`load ${name}: HTTP ${asset.status}`);
      const bytes = await asset.arrayBuffer();
      const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))]
        .map(byte => byte.toString(16).padStart(2, '0')).join('');
      if (hash !== provenance.sha256[`asset/${name}`]) throw Error(`asset changed during capture: ${name}`);
      return new Uint8Array(bytes);
    }, e=>postMessage(e.type === 'result' ? {...e, provenance} : e), data.jit!==false, data.chain===true);
  } catch(e) { postMessage({type:'error',line:e.stack||String(e)}); }
};
