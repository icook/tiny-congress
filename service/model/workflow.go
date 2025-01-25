package model

import (
	"errors"
	"sync"
)

// WorkflowStage represents a stage in the workflow
type WorkflowStage struct {
	Name  string
	Logic func(context map[string]interface{}) (Outcome, error)
}

// Outcome represents the result of a workflow stage
type Outcome struct {
	Success bool
}

// WorkflowManagerImpl implements the WorkflowManager interface
type WorkflowManagerImpl struct {
	stages []WorkflowStage
	mu     sync.RWMutex
}

func NewWorkflowManager() *WorkflowManagerImpl {
	return &WorkflowManagerImpl{
		stages: make([]WorkflowStage, 0),
	}
}

func (w *WorkflowManagerImpl) AddStage(stage WorkflowStage) error {
	w.mu.Lock()
	defer w.mu.Unlock()

	if stage.Name == "" || stage.Logic == nil {
		return errors.New("invalid workflow stage")
	}

	w.stages = append(w.stages, stage)
	return nil
}

func (w *WorkflowManagerImpl) Execute(context map[string]interface{}) (bool, error) {
	w.mu.RLock()
	defer w.mu.RUnlock()

	for _, stage := range w.stages {
		outcome, err := stage.Logic(context)
		if err != nil {
			return false, err
		}
		if !outcome.Success {
			return false, nil
		}
	}
	return true, nil
}
