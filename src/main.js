import { invoke } from '@tauri-apps/api/core';
import './styles.css';

const state = { settings: null, status: null, steps: [] };
const $ = (id) => document.getElementById(id);
const defaultStep = () => ({ action: 'fill', locator_type: 'css', locator: '', value: '', wait_ms: 500 });

function showMessage(text, kind = '') {
  $('message').textContent = text;
  $('message').className = `message ${kind}`;
}

function renderStatus(status) {
  state.status = status;
  const done = status.completed;
  $('status-icon').textContent = done ? '✓' : '○';
  $('status-icon').className = `status-icon ${done ? 'done' : 'pending'}`;
  $('status-text').textContent = done ? '今日已完成打卡' : '今日尚未打卡';
  $('last-checkin').textContent = status.last_completed_at
    ? `最近完成：${status.last_completed_at}`
    : '启动后会自动检查';
  $('status-badge').textContent = done ? '已完成' : '待打卡';
  $('status-badge').className = `status-badge ${done ? 'done' : 'pending'}`;
}

function renderSteps() {
  const container = $('steps');
  container.replaceChildren();
  state.steps.forEach((step, index) => {
    const row = document.createElement('div');
    row.className = 'step-row';

    const valueInputType = step.action === 'fill' ? 'password' : 'text';
    const valuePlaceholder = step.action === 'wait'
      ? '填写内容 / 等待毫秒'
      : '填写内容';

    row.innerHTML = `
      <div class="step-number">${index + 1}</div>
      <select class="step-action">
        <option value="fill">填写</option>
        <option value="click">点击</option>
        <option value="wait">等待</option>
      </select>
      <select class="step-locator-type">
        <option value="css">CSS</option>
        <option value="xpath">XPath</option>
        <option value="text">文本</option>
      </select>
      <input class="step-locator" placeholder="元素定位表达式"/>
      <input class="step-value" type="${valueInputType}" placeholder="${valuePlaceholder}"/>
      <button class="remove-step" title="删除步骤">×</button>
    `;

    // row.innerHTML = `<div class="step-number">${index + 1}</div><select class="step-action"><option value="fill">填写</option><option value="click">点击</option><option value="wait">等待</option></select><select class="step-locator-type"><option value="css">CSS</option><option value="xpath">XPath</option><option value="text">文本</option></select><input class="step-locator" placeholder="元素定位表达式"/><input class="step-value" type="${valueInputType}" placeholder="${valuePlaceholder}"/><button class="remove-step" title="删除步骤">×</button>`;
    // row.innerHTML = `<div class="step-number">${index + 1}</div><select class="step-action"><option value="fill">填写</option><option value="click">点击</option><option value="wait">等待</option></select><select class="step-locator-type"><option value="css">CSS</option><option value="xpath">XPath</option><option value="text">文本</option></select><input class="step-locator" placeholder="元素定位表达式"/><input class="step-value" placeholder="填写内容 / 等待毫秒"/><button class="remove-step" title="删除步骤">×</button>`;
    row.querySelector('.step-action').value = step.action;
    row.querySelector('.step-locator-type').value = step.locator_type || 'css';
    row.querySelector('.step-locator').value = step.locator || '';
    row.querySelector('.step-value').value = step.action === 'wait' ? (step.wait_ms || 500) : (step.value || '');
    row.querySelector('.step-action').addEventListener('change', (event) => { step.action = event.target.value; renderSteps(); });
    row.querySelector('.step-locator-type').addEventListener('change', (event) => { step.locator_type = event.target.value; });
    row.querySelector('.step-locator').addEventListener('input', (event) => { step.locator = event.target.value; });
    row.querySelector('.step-value').addEventListener('input', (event) => { if (step.action === 'wait') step.wait_ms = Number(event.target.value) || 500; else step.value = event.target.value; });
    row.querySelector('.remove-step').addEventListener('click', () => { state.steps.splice(index, 1); renderSteps(); });
    container.appendChild(row);
  });
}

function readSettings() {
  return {
    login_url: $('login-url').value.trim(),
    autostart: $('autostart').checked,
    steps: state.steps.map((step) => ({ action: step.action, locator_type: step.locator_type || 'css', locator: step.locator || '', value: step.value || '', wait_ms: Number(step.wait_ms) || 500 })),
  };
}

async function save() {
  const settings = readSettings();
  if (!settings.login_url) { showMessage('请先填写登录页面链接。', 'error'); return false; }
  if (settings.steps.some((step) => step.action !== 'wait' && !step.locator)) { showMessage('填写或点击步骤必须填写定位表达式。', 'error'); return false; }
  state.settings = await invoke('save_settings', { settings });
  showMessage('设置已保存。', 'success');
  return true;
}

async function load() {
  state.settings = await invoke('get_settings');
  $('login-url').value = state.settings.login_url || '';
  $('autostart').checked = state.settings.autostart;
  state.steps = state.settings.steps?.length ? state.settings.steps : [
    { action: 'fill', locator_type: 'css', locator: '#username', value: '', wait_ms: 500 },
    { action: 'fill', locator_type: 'css', locator: '#password', value: '', wait_ms: 500 },
    { action: 'click', locator_type: 'css', locator: '#login', value: '', wait_ms: 1500 },
    { action: 'click', locator_type: 'css', locator: '#check-in', value: '', wait_ms: 500 },
  ];
  renderSteps();
  renderStatus(await invoke('get_today_status'));
}

$('add-step-button').addEventListener('click', () => { state.steps.push(defaultStep()); renderSteps(); });
$('save-button').addEventListener('click', save);
$('refresh-button').addEventListener('click', async () => renderStatus(await invoke('get_today_status')));
$('run-button').addEventListener('click', async () => {
  try {
    if (await save()) {
      await invoke('open_checkin_window');
      showMessage('浏览器页面已打开，正在执行自动化步骤…', 'success');
    }
  } catch (error) {
    showMessage(`打开页面失败：${error}`, 'error');
  }
});

load().catch((error) => showMessage(`加载失败：${error}`, 'error'));