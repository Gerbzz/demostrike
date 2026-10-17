import './preload-bridge';

// Import after installing the native bridge. The original renderer reads
// window.csdm before it creates React's root.
void import('../upstream/cs-demo-manager/src/ui/renderer');
