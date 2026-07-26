/* Análisis STFT fuera del main thread: radix-2, Hann, sin DOM. */
function fft(re, im) {
  const n = re.length; let j = 0;
  for (let i = 1; i < n; i++) { let bit = n >> 1; while (j & bit) { j ^= bit; bit >>= 1; } j ^= bit; if (i < j) { [re[i],re[j]]=[re[j],re[i]]; [im[i],im[j]]=[im[j],im[i]]; } }
  for (let size = 2; size <= n; size <<= 1) { const a = -2 * Math.PI / size, wr0 = Math.cos(a), wi0 = Math.sin(a); for (let base=0;base<n;base+=size) { let wr=1,wi=0; for(let k=0;k<size/2;k++){const i=base+k,q=i+size/2,tr=re[q]*wr-im[q]*wi,ti=re[q]*wi+im[q]*wr,ar=re[i],ai=im[i];re[i]=ar+tr;im[i]=ai+ti;re[q]=ar-tr;im[q]=ai-ti;const nr=wr*wr0-wi*wi0;wi=wr*wi0+wi*wr0;wr=nr;} } }
}
self.onmessage = ({ data }) => {
  const t0=performance.now(), { samples, sampleRate, fftSize, hopSize, minHz, maxHz, smoothing, focus, aperture }=data;
  const source=new Float32Array(samples), frames=Math.max(1,Math.min(8192,Math.floor(Math.max(0,source.length-fftSize)/hopSize)+1)), bins=Math.min(192,fftSize>>1), image=new Uint8Array(frames*bins), energy=new Float32Array(frames), re=new Float32Array(fftSize), im=new Float32Array(fftSize), lo=Math.max(0,Math.floor(minHz/sampleRate*fftSize)),hi=Math.min(fftSize>>1,Math.ceil(maxHz/sampleRate*fftSize));
  for(let f=0;f<frames;f++){const start=Math.floor(f*(source.length-fftSize)/Math.max(1,frames-1));re.fill(0);im.fill(0);for(let n=0;n<fftSize;n++)re[n]=(source[start+n]||0)*(.5-.5*Math.cos(2*Math.PI*n/(fftSize-1)));fft(re,im);for(let b=0;b<bins;b++){const k=Math.floor(b*(fftSize/2-1)/Math.max(1,bins-1)),m2=(re[k]*re[k]+im[k]*im[k])/(fftSize*fftSize);if(k>=lo&&k<=hi)energy[f]+=m2;const db=10*Math.log10(m2+1e-12);image[f*bins+(bins-1-b)]=Math.max(0,Math.min(255,Math.round((db+90)/90*255)));}}
  // q se condiciona a la misma apertura triangular p del comparador temporal.
  const q=new Float32Array(frames),p=new Float32Array(frames);let sum=0,psum=0;for(let i=0;i<frames;i++){const u=(i+.5)/frames,temporal=Math.max(0,1-Math.abs(u-focus)/Math.max(aperture,.002));p[i]=temporal;psum+=temporal;const v=temporal*Math.pow(energy[i]+1e-12,.85);q[i]=v;sum+=v;}if(psum<=1e-20){p.fill(1/frames)}else for(let i=0;i<frames;i++)p[i]/=psum;if(sum<=1e-20){q.set(p)}else for(let i=0;i<frames;i++)q[i]/=sum;
  const alpha=Math.max(0,Math.min(.95,smoothing));if(alpha){for(let i=1;i<frames;i++)q[i]=alpha*q[i-1]+(1-alpha)*q[i];sum=q.reduce((a,b)=>a+b,0);for(let i=0;i<frames;i++)q[i]/=sum;}
  const cdf=new Float32Array(frames);let acc=0;for(let i=0;i<frames;i++){acc+=q[i];cdf[i]=acc;}cdf[frames-1]=1;
  self.postMessage({type:'analysis',frames,bins,image,q,p,cdf,analysisMs:performance.now()-t0},[image.buffer,q.buffer,p.buffer,cdf.buffer]);
};
