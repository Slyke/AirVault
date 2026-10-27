import { mount } from 'svelte';
import App from './App.svelte';
import './style.css';
import './themes.css';
import { readAppearance, applyAppearance } from './lib/appearance';
applyAppearance({ appearance: readAppearance() });
mount(App, { target: document.getElementById('app')! });
