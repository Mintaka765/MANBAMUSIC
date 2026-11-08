import { mount } from 'svelte';
import App from './App.svelte';
import LyricsWindow from './LyricsWindow.svelte';
import './app.css';

const target = document.getElementById('app')!;

// 同一个前端包同时服务两个窗口：主界面和桌面歌词窗
if (window.location.hash === '#lyrics') {
  // html 上也要一起打标：:root 有底色，只给 body 设置透明不够
  document.documentElement.classList.add('lyrics-window');
  document.body.classList.add('lyrics-window');
  mount(LyricsWindow, { target });
} else {
  mount(App, { target });
}
