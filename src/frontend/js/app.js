// ============================================
// Sincronizador de Calendarios - JavaScript
// Interfaz amigable para usuarios mayores
// ============================================

// Estado de la aplicación
const state = {
    logs: [],
    isRunning: false,
    lastSync: null,
    nextSync: null,
};

// DOM Elements
const elements = {
    statusBadge: document.getElementById('status-badge'),
    lastSync: document.getElementById('last-sync'),
    nextSync: document.getElementById('next-sync'),
    totalReservations: document.getElementById('total-reservations'),
    totalConflicts: document.getElementById('total-conflicts'),
    logContainer: document.getElementById('log-container'),
    configForm: document.getElementById('config-form'),
    tutorial: document.getElementById('tutorial'),
    showTutorial: document.getElementById('show-tutorial'),
    refreshLogs: document.getElementById('refresh-logs'),
    clearLogs: document.getElementById('clear-logs'),
};

// ============================================
// TUTORIAL
// ============================================
elements.showTutorial.addEventListener('click', () => {
    elements.tutorial.classList.toggle('hidden');
    elements.showTutorial.textContent = elements.tutorial.classList.contains('hidden')
        ? '📖 Ver tutorial paso a paso'
        : '📖 Ocultar tutorial';
});

// ============================================
// CONFIGURACIÓN
// ============================================
elements.configForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    
    const config = {
        sync_interval_minutes: parseInt(document.getElementById('sync-interval').value) || 15,
        channels: []
    };

    // Recolectar canales habilitados
    const channels = ['airbnb', 'vrbo', 'booking'];
    channels.forEach(name => {
        const enabled = document.querySelector(`.channel-enabled[data-channel="${name}"]`).checked;
        const url = document.getElementById(`${name}-url`).value.trim();
        
        if (enabled && url) {
            config.channels.push({
                name: name.charAt(0).toUpperCase() + name.slice(1),
                ics_url: url,
                enabled: true
            });
        }
    });

    if (config.channels.length === 0) {
        showToast('⚠️ Por favor, activa al menos un canal y pega su URL.', 'warning');
        return;
    }

    // Guardar configuración (simulado - en producción enviar al backend)
    localStorage.setItem('sync_config', JSON.stringify(config));
    
    showToast('✅ Configuración guardada correctamente', 'success');
    addLog('system', '✅ Configuración actualizada y guardada');
    
    // Simular inicio de sincronización
    startSync();
});

// ============================================
// LOGS
// ============================================
function addLog(type, message) {
    const timestamp = new Date().toLocaleTimeString();
    const entry = { timestamp, type, message };
    state.logs.unshift(entry);
    
    if (state.logs.length > 100) {
        state.logs.pop();
    }
    
    renderLogs();
}

function renderLogs() {
    const container = elements.logContainer;
    container.innerHTML = state.logs.map(log => 
        `<div class="log-entry ${log.type}">[${log.timestamp}] ${log.message}</div>`
    ).join('');
}

elements.refreshLogs.addEventListener('click', renderLogs);

elements.clearLogs.addEventListener('click', () => {
    state.logs = [];
    renderLogs();
    addLog('system', '🗑️ Logs limpiados');
});

// ============================================
// SINCRO - SIMULADO
// ============================================
function startSync() {
    if (state.isRunning) return;
    state.isRunning = true;
    elements.statusBadge.textContent = '🟢 Activo';
    elements.statusBadge.className = 'status-badge status-running';
    
    addLog('system', '🔄 Iniciando sincronización...');
    
    // Simular sincronización
    let count = 0;
    const interval = setInterval(() => {
        count++;
        
        if (count > 10) {
            clearInterval(interval);
            addLog('system', '✅ Sincronización completa');
            return;
        }
        
        const now = new Date();
        elements.lastSync.textContent = now.toLocaleTimeString();
        elements.totalReservations.textContent = Math.floor(Math.random() * 50) + 10;
        elements.totalConflicts.textContent = Math.floor(Math.random() * 3);
        
        const random = Math.random();
        if (random < 0.1) {
            addLog('warning', '⚠️ Conflicto detectado en fechas 15-20 enero');
        } else if (random < 0.05) {
            addLog('error', '❌ Error al conectar con Vrbo - reintentando...');
        } else {
            const channels = ['Airbnb', 'Vrbo', 'Booking'];
            const channel = channels[Math.floor(Math.random() * channels.length)];
            addLog('success', `✅ ${channel} sincronizado correctamente (${Math.floor(Math.random() * 20) + 1} eventos)`);
        }
    }, 2000);
}

// ============================================
// TOAST NOTIFICATIONS
// ============================================
function showToast(message, type = 'info') {
    const toast = document.createElement('div');
    toast.className = `toast toast-${type}`;
    toast.textContent = message;
    toast.style.cssText = `
        position: fixed;
        bottom: 20px;
        right: 20px;
        padding: 16px 24px;
        border-radius: 8px;
        font-size: 1.1rem;
        font-weight: 500;
        background: ${type === 'success' ? '#dcfce7' : type === 'warning' ? '#fef3c7' : '#e0f2fe'};
        color: ${type === 'success' ? '#166534' : type === 'warning' ? '#92400e' : '#1e40af'};
        border: 2px solid ${type === 'success' ? '#86efac' : type === 'warning' ? '#fcd34d' : '#7dd3fc'};
        box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1);
        z-index: 9999;
        max-width: 500px;
        transition: all 0.3s ease;
    `;
    
    document.body.appendChild(toast);
    
    setTimeout(() => {
        toast.style.opacity = '0';
        toast.style.transform = 'translateY(20px)';
        setTimeout(() => toast.remove(), 300);
    }, 4000);
}

// ============================================
// CARGA DE CONFIGURACIÓN GUARDADA
// ============================================
function loadSavedConfig() {
    const saved = localStorage.getItem('sync_config');
    if (saved) {
        try {
            const config = JSON.parse(saved);
            if (config.sync_interval_minutes) {
                document.getElementById('sync-interval').value = config.sync_interval_minutes;
            }
            config.channels.forEach(channel => {
                const id = `${channel.name.toLowerCase()}-url`;
                const input = document.getElementById(id);
                if (input) input.value = channel.ics_url;
            });
        } catch (e) {
            console.warn('Error al cargar configuración guardada');
        }
    }
}

// ============================================
// INICIO
// ============================================
loadSavedConfig();
addLog('system', '🚀 Sistema listo. Configura tus calendarios y presiona "Guardar y Activar"');