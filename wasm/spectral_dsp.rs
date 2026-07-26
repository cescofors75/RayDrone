//! Núcleo STFT sin dependencias para el experimento Spectral Lab.
//! Se prueba en host con `rustc --test spectral_dsp.rs`; la página actual usa
//! un Worker para no bloquear la UI mientras se migra este núcleo a WASM.

use std::f32::consts::PI;

#[derive(Clone, Debug)]
pub struct SpectralAnalysis { pub fft_size: usize, pub hop_size: usize, pub frame_count: usize, pub bin_count: usize, pub magnitudes: Vec<f32>, pub energy_per_frame: Vec<f32>, pub spectral_flux: Vec<f32>, pub harmonicity: Vec<f32> }

pub fn fft(real: &mut [f32], imag: &mut [f32]) {
    let n=real.len(); assert!(n.is_power_of_two() && imag.len()==n);
    let mut j=0; for i in 1..n { let mut bit=n>>1; while j&bit!=0 {j^=bit;bit>>=1} j^=bit; if i<j {real.swap(i,j);imag.swap(i,j)} }
    let mut len=2; while len<=n { let a=-2.0*PI/len as f32; let (wr,wi)=(a.cos(),a.sin()); for base in (0..n).step_by(len) {let(mut ur,mut ui)=(1.,0.);for k in 0..len/2 {let i=base+k;let q=i+len/2;let(tr,ti)=(real[q]*ur-imag[q]*ui,real[q]*ui+imag[q]*ur);let(ar,ai)=(real[i],imag[i]);real[i]=ar+tr;imag[i]=ai+ti;real[q]=ar-tr;imag[q]=ai-ti;let nr=ur*wr-ui*wi;ui=ur*wi+ui*wr;ur=nr}} len*=2; }
}

pub fn analyse(samples:&[f32], fft_size:usize, hop:usize, min_hz:f32, max_hz:f32, sr:f32)->SpectralAnalysis {
    assert!(fft_size.is_power_of_two() && hop>0); let frames=if samples.len()<fft_size {1}else{1+(samples.len()-fft_size)/hop}; let bins=fft_size/2; let mut mag=vec![0.;frames*bins];let mut en=vec![0.;frames];let mut flux=vec![0.;frames];let mut harm=vec![0.;frames];let(lo,hi)=((min_hz*fft_size as f32/sr).max(0.) as usize,(max_hz*fft_size as f32/sr).min(bins as f32) as usize);
    for f in 0..frames {let mut r=vec![0.;fft_size];let mut im=vec![0.;fft_size];for n in 0..fft_size {let x=samples.get(f*hop+n).copied().unwrap_or(0.);r[n]=x*(0.5-0.5*(2.*PI*n as f32/(fft_size-1) as f32).cos())}fft(&mut r,&mut im);let mut total=0.;let mut peak=0.;for k in 0..bins {let m=(r[k]*r[k]+im[k]*im[k]).sqrt();mag[f*bins+k]=m;if k>=lo&&k<=hi {en[f]+=m*m}total+=m;if m>peak{peak=m}if f>0 {flux[f]+=(m-mag[(f-1)*bins+k]).max(0.)}}harm[f]=if total>0.{peak/total}else{0.};} SpectralAnalysis{fft_size,hop_size:hop,frame_count:frames,bin_count:bins,magnitudes:mag,energy_per_frame:en,spectral_flux:flux,harmonicity:harm}
}

pub fn distribution(values:&[f32])->Vec<f32>{let mut out:Vec<f32>=values.iter().map(|v|if v.is_finite(){v.max(0.)}else{0.}).collect();let s:f32=out.iter().sum();if s<=1e-12 {let n=out.len().max(1) as f32;out.fill(1./n)}else{for v in &mut out{*v/=s}}out}
pub fn importance_weight(p:f32,q:f32,floor:f32)->f32{if !p.is_finite()||!q.is_finite(){0.}else{p/q.max(floor.max(1e-12))}}

#[cfg(test)] mod tests { use super::*; #[test]fn sine_bin(){let n=1024;let mut x=(0..n).map(|i|(2.*PI*32.*i as f32/n as f32).sin()).collect::<Vec<_>>();let mut y=vec![0.;n];fft(&mut x,&mut y);let peak=(0..n/2).max_by(|&a,&b|(x[a]*x[a]+y[a]*y[a]).partial_cmp(&(x[b]*x[b]+y[b]*y[b])).unwrap()).unwrap();assert_eq!(peak,32)}#[test]fn distributions_are_safe(){let q=distribution(&[0.,f32::NAN,-1.,2.]);assert!(q.iter().all(|x|x.is_finite()&&*x>=0.));assert!((q.iter().sum::<f32>()-1.).abs()<1e-5);assert!(importance_weight(0.2,0.,1e-5).is_finite())}#[test]fn silence_and_short_stereo_mix(){let a=analyse(&[0.;32],64,16,0.,20000.,48000.);assert_eq!(a.frame_count,1);assert!(a.energy_per_frame[0].is_finite());let mono=[0.2,-0.2,0.4];let stereo=mono.iter().zip(mono.iter()).map(|(l,r)|(l+r)*0.5).collect::<Vec<_>>();assert_eq!(mono.to_vec(),stereo)}}
